//! Out-of-process ChatGPT Codex login webview.
//!
//! chatgpt.com sign-in and its post-login quota request run in this disposable process so a
//! stalled WebView2 page cannot block the main application's shared UI thread. The helper retains
//! WebView2's normal browser identity. After its in-page usage request succeeds, it reads the
//! chatgpt.com cookies once and returns them to the parent over stdout. The parent independently
//! validates the cookie against the same usage endpoint before saving it.

use std::path::PathBuf;
use std::time::Duration;

use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::window::WindowBuilder;
use wry::{WebContext, WebView, WebViewBuilder};

const LOGIN_URL: &str = "https://chatgpt.com/auth/login";
const COOKIE_URL: &str = "https://chatgpt.com";

// This runs in the authenticated chatgpt.com document. A successful same-origin request proves
// that the browser profile has a usable session before we invoke the one native cookie read.
const LOGIN_JS: &str = r#"(function(){
  function post(m){ try{ window.ipc.postMessage(m); }catch(_){} }
  var finished=false, pending=false;
  var timer=setInterval(async function(){
    if(finished||pending||location.hostname!=="chatgpt.com") return;
    pending=true;
    try {
      var response=await fetch("/backend-api/wham/usage", {
        credentials:"include",
        headers:{"Accept":"application/json","OAI-App-Brand":"codex"}
      });
      if(response.ok){ finished=true; clearInterval(timer); post("USAGE_OK"); }
    } catch(_) {
    } finally {
      pending=false;
    }
  }, 1500);
})();"#;

#[derive(Debug)]
enum Msg {
    Ipc(String),
    Timeout,
}

/// Entry point for the isolated Codex login process. Prints one `SM_RESULT` line for the parent.
pub fn run(mode: &str) {
    if mode != "login" {
        emit("ERROR unsupported mode");
        return;
    }
    let Some(udf) = std::env::var_os("SM_CODEX_UDF").map(PathBuf::from) else {
        emit("ERROR missing profile");
        return;
    };

    let event_loop = EventLoopBuilder::<Msg>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let window = match WindowBuilder::new()
        .with_title("Sign in to Codex")
        .with_inner_size(tao::dpi::LogicalSize::new(520.0, 760.0))
        .build(&event_loop)
    {
        Ok(window) => window,
        Err(error) => {
            emit(&format!("ERROR window {error}"));
            return;
        }
    };

    let mut web_context = WebContext::new(Some(udf));
    let ipc_proxy = proxy.clone();
    let webview = match WebViewBuilder::new_with_web_context(&mut web_context)
        .with_url(LOGIN_URL)
        .with_initialization_script(LOGIN_JS)
        .with_ipc_handler(move |req: wry::http::Request<String>| {
            let _ = ipc_proxy.send_event(Msg::Ipc(req.into_body()));
        })
        .build(&window)
    {
        Ok(webview) => webview,
        Err(error) => {
            emit(&format!("ERROR webview {error}"));
            return;
        }
    };

    let timeout_proxy = proxy.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(290));
        let _ = timeout_proxy.send_event(Msg::Timeout);
    });

    event_loop.run(move |event, _target, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::UserEvent(Msg::Ipc(body)) if body == "USAGE_OK" => {
                // Native cookie access is deliberately deferred until a successful in-page
                // usage fetch, and is performed exactly once per helper run.
                match cookie_header(&webview) {
                    Some(cookie) => emit(&format!("COOKIE {cookie}")),
                    None => emit("ERROR cookies unavailable"),
                }
                *control_flow = ControlFlow::Exit;
            }
            Event::UserEvent(Msg::Timeout) => {
                emit("TIMEOUT");
                *control_flow = ControlFlow::Exit;
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                emit("CANCELLED");
                *control_flow = ControlFlow::Exit;
            }
            _ => {}
        }
    });
}

fn cookie_header(webview: &WebView) -> Option<String> {
    let pairs = webview
        .cookies_for_url(COOKIE_URL)
        .ok()?
        .into_iter()
        .filter_map(|cookie| {
            let name = cookie.name();
            let value = cookie.value();
            (!name.is_empty() && header_component_is_safe(name) && header_component_is_safe(value))
                .then(|| format!("{name}={value}"))
        })
        .collect::<Vec<_>>();
    (!pairs.is_empty()).then(|| pairs.join("; "))
}

fn header_component_is_safe(value: &str) -> bool {
    !value.contains(['\r', '\n'])
}

/// Print the single machine-readable result line and flush stdout, which is piped by the parent.
fn emit(payload: &str) {
    use std::io::Write;

    let mut out = std::io::stdout();
    let _ = writeln!(out, "SM_RESULT {payload}");
    let _ = out.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookie_header_filters_control_characters() {
        assert!(!header_component_is_safe("session\rvalue"));
        assert!(!header_component_is_safe("session\nvalue"));
        assert!(header_component_is_safe("session-value"));
    }
}
