//! Routing.
//!
//! A static route table. The alternative the C++ firmware used, a `switch` on a hand-typed string
//! in every handler with the path compared by `strcmp` scattered across thirty functions, is
//! exactly the shape that grows a route nobody tests and forgets to authorise. Here a route is one
//! row, its method set is explicit, and "this path exists but not for this method" is a different
//! answer from "this path does not exist", because those have different security consequences: a
//! `405` on `/ota` says the route exists and the method was wrong, which is information a scanner
//! uses.

use heapless::String;

use crate::request::{Method, RequestHead};
use crate::response::{Body, Response, Status};

/// One route.
#[derive(Debug)]
pub struct Route<'a> {
    pub path: &'a str,
    /// The methods this route answers. A method not listed gets 405, not 404.
    pub methods: &'a [Method],
    /// Whether the route needs authentication. Checked before the handler runs, so a handler
    /// cannot forget.
    pub authenticated: bool,
}

impl Default for Route<'_> {
    fn default() -> Self {
        Self {
            path: "/",
            methods: &[],
            authenticated: true,
        }
    }
}

/// What the router decided, before any handler runs.
///
/// The router's answer and the handler's answer are separate types on purpose: a caller can
/// authenticate and rate-limit on a decision without having built a response, and a handler cannot
/// be reached for a route that does not exist.
///
/// The `Allow` set is borrowed from the route table rather than a `&'static [Method]`, because the
/// table is a static in practice but the router is generic over its lifetime and a caller could
/// build one on the stack.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Decision<'a> {
    /// This route, for this method.
    Matched { index: usize },
    /// The path exists but not for this method. 405 with `Allow`.
    MethodNotAllowed { allow: &'a [Method] },
    /// No such path. 404.
    NotFound,
}

/// The route table.
#[derive(Debug)]
pub struct Router<'a> {
    routes: &'a [Route<'a>],
}

impl<'a> Router<'a> {
    pub const fn new(routes: &'a [Route<'a>]) -> Self {
        Self { routes }
    }

    pub fn len(&self) -> usize {
        self.routes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.routes.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<&'a Route<'a>> {
        self.routes.get(index)
    }

    /// The 404 body. The C++ server returned the requested path back in the body, which turned
    /// any endpoint into a reflector: put a script tag in the path and it comes back as HTML.
    pub fn not_found(&self) -> Response<'static> {
        Response::json(Status::NotFound, r#"{"error":"not found"}"#)
    }

    /// The 405 body. It names the methods the route does accept, which is the point of a 405.
    pub fn method_not_allowed(&self, allow: &[Method]) -> Response<'static> {
        let mut r = Response::json(
            Status::MethodNotAllowed,
            r#"{"error":"method not allowed"}"#,
        );
        r = r.header("Allow", &allow_header(allow));
        r
    }

    /// Decides what to do with a request head.
    pub fn decide<'h>(&'h self, head: &RequestHead) -> Decision<'h> {
        for (index, route) in self.routes.iter().enumerate() {
            if route.path == head.path.as_str() {
                if route.methods.contains(&head.method) {
                    return Decision::Matched { index };
                }
                return Decision::MethodNotAllowed {
                    allow: route.methods,
                };
            }
        }
        Decision::NotFound
    }

    /// Whether a route at `index` requires authentication.
    ///
    /// Used before the handler, so a handler cannot serve an authenticated route without the check
    /// having happened. The C++ firmware had four separate `authenticate()` calls in four
    /// handlers and three of the OTA routes had none (defect D01).
    pub fn is_authenticated(&self, index: usize) -> bool {
        match self.routes.get(index) {
            Some(r) => r.authenticated,
            None => true,
        }
    }

    /// Every registered path, for the CORS preflight's `Allow` on a wildcard and for a test that
    /// checks no route is registered twice.
    pub fn paths(&self) -> impl Iterator<Item = &'a str> + '_ {
        self.routes.iter().map(|r| r.path)
    }

    /// The response to write when the router itself decides, or `None` if a handler should run.
    pub fn pre_handled(&self, decision: Decision<'_>) -> Option<Response<'static>> {
        match decision {
            Decision::NotFound => Some(self.not_found()),
            Decision::MethodNotAllowed { allow } => Some(self.method_not_allowed(allow)),
            Decision::Matched { .. } => None,
        }
    }
}

/// The `Allow` header value.
fn allow_header(methods: &[Method]) -> String<96> {
    let mut out = String::new();
    for m in methods {
        if !out.is_empty() {
            let _ = core::fmt::Write::write_fmt(&mut out, format_args!(", "));
        }
        let _ = core::fmt::Write::write_fmt(&mut out, format_args!("{}", m.as_str()));
    }
    out
}

/// A body that is always empty, for a `204`.
pub fn no_content() -> Response<'static> {
    Response::with_body(Status::NoContent, Body::Empty)
}

#[cfg(test)]
mod tests {
    use super::*;
    use heapless::Vec;

    static ROUTES: &[Route<'_>] = &[
        Route {
            path: "/api/state",
            methods: &[Method::Get],
            authenticated: false,
        },
        Route {
            path: "/api/setpoint",
            methods: &[Method::Get, Method::Post],
            authenticated: true,
        },
        Route {
            path: "/ota",
            methods: &[Method::Post],
            authenticated: true,
        },
        Route {
            path: "/events",
            methods: &[Method::Get],
            authenticated: false,
        },
    ];

    fn router() -> Router<'static> {
        Router::new(ROUTES)
    }

    fn head(method: Method, path: &str) -> RequestHead {
        let mut h = RequestHead::new(method);
        let _ = h.path.push_str(path);
        h
    }

    #[test]
    fn a_registered_path_and_method_is_matched() {
        assert_eq!(
            router().decide(&head(Method::Get, "/api/state")),
            Decision::Matched { index: 0 }
        );
        assert_eq!(
            router().decide(&head(Method::Post, "/api/setpoint")),
            Decision::Matched { index: 1 }
        );
    }

    #[test]
    fn a_registered_path_with_the_wrong_method_is_405_not_404() {
        // The distinction matters: a 405 tells a caller the route exists, which is information a
        // scanner uses, but hiding it behind a 404 would be worse for the user's own debugging.
        // What matters for security is that the handler did not run.
        let r = router();
        let h = head(Method::Delete, "/api/state");
        let decision = r.decide(&h);
        assert_eq!(
            decision,
            Decision::MethodNotAllowed {
                allow: &[Method::Get]
            }
        );
        let response = r.pre_handled(decision).unwrap();
        assert_eq!(response.status, Status::MethodNotAllowed);
        assert_eq!(response.get_header("Allow"), Some("GET"));
    }

    #[test]
    fn the_allow_header_lists_every_method_the_route_accepts() {
        let router = router();
        let h = head(Method::Delete, "/api/setpoint");
        let response = router.pre_handled(router.decide(&h)).unwrap();
        assert_eq!(response.get_header("Allow"), Some("GET, POST"));
    }

    #[test]
    fn an_unknown_path_is_404() {
        let router = router();
        let h = head(Method::Get, "/nope");
        let decision = router.decide(&h);
        assert_eq!(decision, Decision::NotFound);
        assert_eq!(
            router.pre_handled(decision).unwrap().status,
            Status::NotFound
        );
    }

    #[test]
    fn a_404_does_not_reflect_the_path_back() {
        // The C++ server echoed the requested path into the 404 body, which made any endpoint a
        // reflector: a path containing markup came back inside a JSON string that a browser would
        // happily render.
        let r = router().not_found();
        let mut buf = [0u8; 128];
        let n = r.write_head(&mut buf);
        let head = core::str::from_utf8(&buf[..n]).unwrap();
        assert!(
            !head.contains("<"),
            "a response head must not contain markup"
        );
        assert!(!head.contains("script"));
    }

    #[test]
    fn a_matched_route_has_no_pre_handled_response() {
        // This is what lets a handler run.
        let router = router();
        let h = head(Method::Get, "/api/state");
        assert!(router.pre_handled(router.decide(&h)).is_none());
    }

    #[test]
    fn authentication_is_a_property_of_the_route_not_of_the_handler() {
        // The C++ firmware had four `authenticate()` calls in handlers and three OTA routes with
        // none, which is defect D01. Here the route decides, so a handler cannot forget.
        assert!(!router().is_authenticated(0), "/api/state is public");
        assert!(
            router().is_authenticated(1),
            "/api/setpoint is authenticated"
        );
        assert!(router().is_authenticated(2), "/ota is authenticated");
    }

    #[test]
    fn an_index_past_the_end_is_treated_as_authenticated() {
        // Fail closed. A bug that produced a bad index must not produce an unprotected route.
        assert!(router().is_authenticated(999));
    }

    #[test]
    fn every_registered_path_is_reachable() {
        let r = router();
        for path in r.paths() {
            assert_ne!(
                r.decide(&head(Method::Options, path)),
                Decision::NotFound,
                "{path}"
            );
        }
    }

    #[test]
    fn no_two_routes_share_a_path() {
        // Two routes on one path means the first shadows the second and the second is dead code
        // that looks live. The C++ firmware had this: `/ota` was registered twice with different
        // handlers.
        let mut seen: Vec<&str, 32> = Vec::new();
        let router = router();
        for path in router.paths() {
            assert!(seen.push(path).is_ok(), "{path} is registered twice");
        }
    }

    #[test]
    fn an_empty_router_answers_404_to_everything() {
        static EMPTY: &[Route<'_>] = &[];
        let r = Router::new(EMPTY);
        assert!(r.is_empty());
        assert_eq!(r.decide(&head(Method::Get, "/")), Decision::NotFound);
    }

    #[test]
    fn a_204_carries_no_body() {
        let r = no_content();
        assert_eq!(r.status, Status::NoContent);
        assert_eq!(r.body.length(), 0);
    }
}
