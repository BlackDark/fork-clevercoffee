fn main() {
    let ports = serialport::available_ports().unwrap_or_default();
    println!("{} ports", ports.len());
    for p in ports {
        println!("{:?}", p.port_name);
    }
}
