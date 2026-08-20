//! Out-of-process ChatGPT Codex login webview.
//!
//! chatgpt.com sign-in runs in this disposable process so a stalled WebView2 page cannot block
//! the main application's shared UI thread. The helper retains WebView2's normal browser identity.
//! Once its in-page session endpoint exposes an OAuth token, it reads the chatgpt.com cookies once
//! and returns them to the parent over stdout. The parent independently validates the cookie with
//! a fresh OAuth bearer request before saving it.

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::window::WindowBuilder;
use wry::{WebContext, WebView, WebViewBuilder};

#[cfg(windows)]
use webview2_com::{
    take_pwstr, GetCookiesCompletedHandler,
    Microsoft::Web::WebView2::Win32::{ICoreWebView2CookieList, ICoreWebView2_2},
};
#[cfg(windows)]
use windows::core::{Interface, HSTRING, PCWSTR, PWSTR};
#[cfg(windows)]
use wry::WebViewExtWindows;

const LOGIN_URL: &str = "https://chatgpt.com/auth/login";
const COOKIE_URL: &str = "https://chatgpt.com";
const COOKIE_CAPTURE_TIMEOUT: Duration = Duration::from_secs(10);

// This runs in the authenticated top-level chatgpt.com document. `/api/auth/session` can return
// 200 before sign-in, so a non-empty access token is the actual completion signal. The parent
// later revalidates the returned cookie against WHAM with bearer authentication before saving it.
const LOGIN_JS: &str = r#"(function(){
  function post(m){ try{ window.ipc.postMessage(m); }catch(_){} }
  var finished=false, pending=false, timer=0;
  async function probe(){
    if(finished||pending||window.top!==window||location.origin!=="https://chatgpt.com") return;
    pending=true;
    var controller=new AbortController();
    var abortTimer=setTimeout(function(){ controller.abort(); }, 10000);
    try {
      var response=await fetch("/api/auth/session", {
        credentials:"include",
        headers:{"Accept":"application/json"},
        signal:controller.signal
      });
      if(!response.ok) return;
      var session=await response.json();
      var token=typeof session.accessToken==="string" ? session.accessToken : session.access_token;
      if(typeof token==="string"&&token.length>0){
        finished=true;
        clearInterval(timer);
        post(JSON.stringify({type:"SESSION_READY",userAgent:navigator.userAgent}));
      }
    } catch(_) {
    } finally {
      clearTimeout(abortTimer);
      pending=false;
    }
  }
  probe();
  timer=setInterval(probe,1500);
})();"#;

#[derive(Debug)]
enum Msg {
    Ipc(String),
    Cookies(Result<Option<String>, String>),
    CookieTimeout,
    Timeout,
}

#[derive(Deserialize)]
struct IpcMessage {
    #[serde(rename = "type")]
    kind: String,
    #[serde(rename = "userAgent", default)]
    user_agent: String,
}

#[derive(Serialize)]
struct CookiePayload {
    cookie: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    user_agent: Option<String>,
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
            // The init script is injected into subframes too. Only a top-level chatgpt.com
            // document may ask this helper to capture an authenticated cookie.
            let trusted_source =
                req.uri().scheme_str() == Some("https") && req.uri().host() == Some("chatgpt.com");
            if trusted_source {
                let _ = ipc_proxy.send_event(Msg::Ipc(req.into_body()));
            }
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

    let mut cookie_capture_pending = false;
    let mut cookie_user_agent = None;
    event_loop.run(move |event, _target, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::UserEvent(Msg::Ipc(body)) if !cookie_capture_pending => {
                let Some(user_agent) = session_ready_user_agent(&body) else {
                    return;
                };
                cookie_capture_pending = true;
                cookie_user_agent = user_agent;
                if let Err(error) = begin_cookie_capture(&webview, proxy.clone()) {
                    emit(&format!("ERROR cookies unavailable: {error}"));
                    *control_flow = ControlFlow::Exit;
                    return;
                }
                let timeout_proxy = proxy.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(COOKIE_CAPTURE_TIMEOUT);
                    let _ = timeout_proxy.send_event(Msg::CookieTimeout);
                });
            }
            Event::UserEvent(Msg::Cookies(result)) if cookie_capture_pending => {
                match result {
                    Ok(Some(cookie)) => emit_cookie(cookie, cookie_user_agent.take()),
                    Ok(None) => emit("ERROR cookies unavailable"),
                    Err(error) => emit(&format!("ERROR cookies unavailable: {error}")),
                }
                *control_flow = ControlFlow::Exit;
            }
            Event::UserEvent(Msg::CookieTimeout) if cookie_capture_pending => {
                emit("ERROR cookie capture timed out");
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

fn session_ready_user_agent(body: &str) -> Option<Option<String>> {
    let message = serde_json::from_str::<IpcMessage>(body).ok()?;
    (message.kind == "SESSION_READY")
        .then(|| is_safe_user_agent(&message.user_agent).then_some(message.user_agent))
}

#[cfg(windows)]
fn begin_cookie_capture(webview: &WebView, proxy: EventLoopProxy<Msg>) -> Result<(), String> {
    let native = webview.webview();
    let manager = unsafe {
        native
            .cast::<ICoreWebView2_2>()
            .map_err(|error| error.to_string())?
            .CookieManager()
            .map_err(|error| error.to_string())?
    };
    // The WebView2 request is asynchronous. Capturing the HSTRING in the completion closure
    // keeps its buffer alive until WebView2 has completed with the URL.
    let uri = HSTRING::from(COOKIE_URL);
    let uri_for_callback = uri.clone();
    let handler = GetCookiesCompletedHandler::create(Box::new(move |status, list| {
        let _keep_uri_alive = &uri_for_callback;
        let result = (|| -> windows::core::Result<Option<String>> {
            status?;
            cookie_header_from_list(list)
        })()
        .map_err(|error| error.to_string());
        let _ = proxy.send_event(Msg::Cookies(result));
        Ok(())
    }));
    unsafe {
        manager
            .GetCookies(PCWSTR::from_raw(uri.as_ptr()), &handler)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(windows)]
fn cookie_header_from_list(
    list: Option<ICoreWebView2CookieList>,
) -> windows::core::Result<Option<String>> {
    let Some(list) = list else {
        return Ok(None);
    };
    let mut count = 0;
    unsafe { list.Count(&mut count)? };
    let mut pairs = Vec::with_capacity(count as usize);
    for index in 0..count {
        let cookie = unsafe { list.GetValueAtIndex(index)? };
        let mut name = PWSTR::null();
        unsafe { cookie.Name(&mut name)? };
        let name = take_pwstr(name);
        let mut value = PWSTR::null();
        unsafe { cookie.Value(&mut value)? };
        let value = take_pwstr(value);
        if !name.is_empty() && header_component_is_safe(&name) && header_component_is_safe(&value) {
            pairs.push(format!("{name}={value}"));
        }
    }
    Ok((!pairs.is_empty()).then(|| pairs.join("; ")))
}

#[cfg(not(windows))]
fn begin_cookie_capture(webview: &WebView, proxy: EventLoopProxy<Msg>) -> Result<(), String> {
    let result = cookie_header(webview);
    proxy
        .send_event(Msg::Cookies(Ok(result)))
        .map_err(|error| error.to_string())
}

#[cfg(not(windows))]
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

fn is_safe_user_agent(value: &str) -> bool {
    !value.is_empty() && value.len() <= 512 && header_component_is_safe(value)
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

fn emit_cookie(cookie: String, user_agent: Option<String>) {
    let payload = CookiePayload { cookie, user_agent };
    match serde_json::to_string(&payload) {
        Ok(serialized) => emit(&format!("COOKIE {serialized}")),
        Err(_) => emit("ERROR cookie payload encoding failed"),
    }
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

    #[test]
    fn accepts_only_authenticated_top_level_session_messages() {
        assert_eq!(
            session_ready_user_agent(
                r#"{"type":"SESSION_READY","userAgent":"Mozilla/5.0 WebView2"}"#
            ),
            Some(Some("Mozilla/5.0 WebView2".to_string()))
        );
        assert_eq!(
            session_ready_user_agent(r#"{"type":"SESSION_READY","userAgent":"bad\nagent"}"#),
            Some(None)
        );
        assert!(session_ready_user_agent(r#"{"type":"OTHER"}"#).is_none());
        assert!(session_ready_user_agent("not-json").is_none());
    }
}
