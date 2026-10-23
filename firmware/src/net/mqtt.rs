//! MQTT client: publishes the [`cc_mqtt::ENTITIES`] (only changed values), applies
//! `<name>/set` messages, sends Home Assistant discovery and keeps `status`
//! (`online`, last will `offline`) current.

use core::fmt::Write as _;
use core::net::{IpAddr, Ipv4Addr, SocketAddr};

use cc_api::json::Writer;
use cc_core::{Command, Config};
use cc_mqtt::{ChangeFilter, ENTITIES, Incoming};
use embassy_futures::select::{Either, select};
use embassy_net::Stack;
use embassy_net::dns::DnsQueryType;
use embassy_net::tcp::TcpSocket;
use embassy_time::{Duration, Instant, Timer};
use heapless::String;
use log::{info, warn};
use rust_mqtt::Bytes;
use rust_mqtt::buffer::AllocBuffer;
use rust_mqtt::client::Client;
use rust_mqtt::client::event::Event;
use rust_mqtt::client::options::{
    ConnectOptions, PublicationOptions, SubscriptionOptions, TopicReference, WillOptions,
};
use rust_mqtt::config::KeepAlive;
use rust_mqtt::types::{MqttBinary, MqttString, QoS, TopicFilter, TopicName};

use crate::shared;

type Mqtt<'a> = Client<'a, 'a, TcpSocket<'a>, AllocBuffer, 1, 1, 1, 0, 0, 0, 0>;

const KEEP_ALIVE_S: u16 = 30;
const RETRY: Duration = Duration::from_secs(10);

fn mqtt_str(s: &str) -> Option<MqttString<'_>> {
    MqttString::try_from(s).ok()
}

fn topic_name(s: &str) -> Option<TopicName<'_>> {
    TopicName::new(mqtt_str(s)?)
}

async fn resolve(stack: Stack<'static>, host: &str) -> Option<IpAddr> {
    if let Ok(ip) = host.parse::<Ipv4Addr>() {
        return Some(IpAddr::V4(ip));
    }
    let addrs = stack.dns_query(host, DnsQueryType::A).await.ok()?;
    addrs.first().map(|&a| IpAddr::from(a))
}

#[embassy_executor::task]
pub async fn task(stack: Stack<'static>) -> ! {
    let mut rx = alloc::vec![0u8; 1024];
    let mut tx = alloc::vec![0u8; 2048];
    loop {
        stack.wait_config_up().await;
        let cfg = shared::with(|s| s.cfg.clone());
        let Some(ip) = resolve(stack, &cfg.mqtt_broker).await else {
            warn!("mqtt: cannot resolve {}", cfg.mqtt_broker);
            Timer::after(RETRY).await;
            continue;
        };
        let mut socket = TcpSocket::new(stack, &mut rx, &mut tx);
        socket.set_timeout(Some(Duration::from_secs(u64::from(KEEP_ALIVE_S) * 2)));
        let port = u16::try_from(cfg.mqtt_port).unwrap_or(1883);
        if let Err(e) = socket.connect(SocketAddr::new(ip, port)).await {
            warn!("mqtt: connect to {ip}:{port} failed: {e:?}");
            Timer::after(RETRY).await;
            continue;
        }
        let r = session(socket, &cfg).await;
        shared::with(|s| s.net.mqtt_connected = false);
        warn!("mqtt: session ended ({r:?})");
        Timer::after(RETRY).await;
    }
}

#[derive(Debug)]
enum End {
    Connect,
    Io,
}

async fn session(socket: TcpSocket<'_>, cfg: &Config) -> Result<(), End> {
    let mut status_topic: String<128> = String::new();
    let _ = cc_mqtt::topic(cfg, "status", None, &mut status_topic);
    let mut client_id: String<48> = String::new();
    let _ = write!(client_id, "clevercoffee-{}", cfg.system_hostname);

    let mut buffer = AllocBuffer;
    let mut client: Mqtt<'_> = Client::new(&mut buffer);
    let will_topic = topic_name(&status_topic).ok_or(End::Connect)?;
    let mut options = ConnectOptions::new()
        .clean_start()
        .keep_alive(KeepAlive::Seconds(
            core::num::NonZero::new(KEEP_ALIVE_S).unwrap_or(core::num::NonZero::<u16>::MIN),
        ))
        .will(
            WillOptions::new(
                will_topic,
                MqttBinary::try_from("offline").map_err(|_| End::Connect)?,
            )
            .retain(),
        );
    if !cfg.mqtt_username.is_empty() {
        options = options.user_name(mqtt_str(&cfg.mqtt_username).ok_or(End::Connect)?);
        options =
            options.password(MqttBinary::try_from(cfg.mqtt_password.as_str()).map_err(|_| End::Connect)?);
    }
    client
        .connect(socket, &options, mqtt_str(&client_id))
        .await
        .map_err(|e| {
            warn!("mqtt: rejected: {e:?}");
            End::Connect
        })?;
    info!("mqtt: connected");
    shared::with(|s| s.net.mqtt_connected = true);

    publish(&mut client, &status_topic, "online", true).await?;
    let mut filter_topic: String<128> = String::new();
    let _ = cc_mqtt::topic(cfg, "+", Some("set"), &mut filter_topic);
    let filter = TopicFilter::new(mqtt_str(&filter_topic).ok_or(End::Connect)?).ok_or(End::Connect)?;
    client
        .subscribe(filter, &SubscriptionOptions::new())
        .await
        .map_err(|_| End::Io)?;

    let mut changes = ChangeFilter::default();
    let mut next_publish = Instant::now();
    let mut next_discovery = Instant::now();
    let mut keep_alive = cc_mqtt::KeepAlive::new(KEEP_ALIVE_S, Instant::now().as_millis());
    loop {
        let now = Instant::now();
        if now >= next_publish {
            let (interval, sent) = publish_changed(&mut client, &mut changes).await?;
            next_publish = now + Duration::from_millis(interval);
            if sent {
                keep_alive.sent(now.as_millis());
            }
        }
        if now >= next_discovery {
            let hassio = shared::with(|s| s.cfg.mqtt_hassio_enabled);
            if hassio && discovery(&mut client).await? {
                keep_alive.sent(now.as_millis());
            }
            next_discovery = now + Duration::from_millis(cc_mqtt::DISCOVERY_INTERVAL_MS);
        }
        if keep_alive.ping_due(now.as_millis()) {
            client.ping().await.map_err(|_| End::Io)?;
            keep_alive.sent(now.as_millis());
        }
        let ping_at = Instant::from_millis(keep_alive.ping_at_ms());
        let wait = Timer::at(next_publish.min(next_discovery).min(ping_at));
        match select(client.poll_header(), wait).await {
            Either::First(header) => {
                let header = header.map_err(|_| End::Io)?;
                if let Event::Publish(p) = client.poll_body(header).await.map_err(|_| End::Io)? {
                    let topic: &str = p.topic.name().map_or("", |n| n.as_ref().as_ref());
                    let payload = core::str::from_utf8(&p.message).unwrap_or("");
                    handle_incoming(topic, payload);
                    // Answer quickly so the sender sees the new state.
                    next_publish = Instant::now();
                }
            }
            Either::Second(()) => {}
        }
    }
}

async fn publish(client: &mut Mqtt<'_>, topic: &str, payload: &str, retain: bool) -> Result<(), End> {
    let Some(name) = topic_name(topic) else {
        return Ok(());
    };
    let mut opts = PublicationOptions::new(TopicReference::Name(name)).qos(QoS::AtMostOnce);
    if retain {
        opts = opts.retain();
    }
    client
        .publish(&opts, Bytes::from(payload))
        .await
        .map(|_| ())
        .map_err(|_| End::Io)
}

/// Publishes entities whose payload changed; returns the next interval and whether
/// anything was sent.
async fn publish_changed(client: &mut Mqtt<'_>, changes: &mut ChangeFilter) -> Result<(u64, bool), End> {
    let mut interval = cc_mqtt::INTERVAL_MS;
    let mut sent = false;
    for (i, e) in ENTITIES.iter().enumerate() {
        let mut topic: String<128> = String::new();
        let mut payload: String<64> = String::new();
        let due = shared::with(|s| {
            if !e.needs.met(&s.cfg) {
                return false;
            }
            let st = s.machine.status(&s.cfg);
            interval = cc_mqtt::interval_ms(&st);
            let _ = cc_mqtt::topic(&s.cfg, e.topic, None, &mut topic);
            let _ = cc_mqtt::payload(e, &s.cfg, &st, &mut payload);
            true
        });
        if due && changes.changed(i, &payload) {
            publish(client, &topic, &payload, false).await?;
            sent = true;
        }
    }
    Ok((interval, sent))
}

/// Publishes the Home Assistant discovery; `true` if anything was sent.
async fn discovery(client: &mut Mqtt<'_>) -> Result<bool, End> {
    let mut sent = false;
    let mut body = alloc::string::String::new();
    for e in ENTITIES {
        let mut topic: String<160> = String::new();
        body.clear();
        let ok = shared::with(|s| {
            if !e.needs.met(&s.cfg) {
                return false;
            }
            matches!(cc_mqtt::discovery_topic(&s.cfg, e, &mut topic), Some(Ok(())))
                && matches!(
                    cc_mqtt::discovery_payload(&s.cfg, e, &mut Writer::new(&mut body)),
                    Some(Ok(()))
                )
        });
        if ok {
            publish(client, &topic, &body, true).await?;
            sent = true;
        }
    }
    Ok(sent)
}

fn handle_incoming(topic: &str, payload: &str) {
    let persist = shared::with(|s| {
        let mut ev = cc_core::machine::Events::new();
        match cc_mqtt::parse_incoming(&s.cfg, topic, payload) {
            Some(Incoming::Param { key, value }) => match s.cfg.set_from_str(key, value) {
                Ok(true) => {
                    s.machine.command(Command::ConfigUpdated, &mut s.cfg, &mut ev);
                    true
                }
                Ok(false) => false,
                Err(e) => {
                    warn!("mqtt: {key}={value} refused: {e}");
                    false
                }
            },
            Some(Incoming::Command(cmd)) => {
                s.machine.command(cmd, &mut s.cfg, &mut ev);
                !ev.is_empty()
            }
            None => false,
        }
    });
    if persist {
        shared::PERSIST.signal(());
    }
}
