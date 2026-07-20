//! Out-of-process Gemini webview helper.
//!
//! Google refuses to render its sign-in inside an embedded webview, and worse, loading the
//! Google login page in the app's shared WebView2 UI thread wedges it and freezes the WHOLE
//! app (every window in one process shares one Win32 UI/message-loop thread). The fix is
//! process isolation: the main app relaunches its own binary with `SM_GEMINI_MODE=login|scrape`,
//! and THIS module runs a bare `tao` + `wry` single-window app in that separate process. If the
//! Google page wedges the webview's UI thread, only this disposable helper process freezes; the
//! main app keeps running and simply kills the helper on timeout.
//!
//! It uses a dedicated WebView2 user-data-folder (`SM_GEMINI_UDF`) so the captured Google session
//! persists across login->scrape invocations and never touches the main app's WebView2 profile.
//! The webview talks back to Rust over wry's IPC channel (`window.ipc.postMessage`), and the
//! helper prints a single `SM_RESULT <payload>` line to stdout for the parent to read.

use std::path::PathBuf;
use std::time::Duration;

use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::window::WindowBuilder;
use wry::{WebContext, WebViewBuilder};

/// Firefox UA: Firefox does not send `Sec-CH-UA` client hints, so paired with `UserAgentClientHint`
/// disabled (WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS, set in lib.rs) it removes the UA/client-hint
/// mismatch Google uses to reject embedded browsers. Fragile, ToS-gray; degrades to a timeout.
pub const FIREFOX_UA: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:140.0) Gecko/20100101 Firefox/140.0";

const HOME_URL: &str = "https://gemini.google.com/app";
const USAGE_URL: &str = "https://gemini.google.com/usage";

// Injected before page scripts on EVERY document. Detects a completed sign-in: we are on
// gemini.google.com with NO sign-in affordance to the Google account chooser. The signed-OUT
// landing page ALSO stays on gemini.google.com, so "still on gemini" alone is a false positive;
// the discriminator is the presence of an `accounts.google.com`/`ServiceLogin` sign-in link, which
// only the logged-out shell has. Require the signed-in state to hold ~2s to avoid firing mid-load.
const LOGIN_JS: &str = r#"(function(){
  function post(m){ try{ window.ipc.postMessage(m); }catch(e){} }
  var stable=0;
  var iv=setInterval(function(){
    try{
      if(/accounts\.google\.com/.test(location.href)){ stable=0; return; }   // on the sign-in flow
      if(location.hostname.indexOf('gemini.google.com')===-1){ stable=0; return; }
      // Signed-in Google surfaces show the account button, whose link targets
      // accounts.google.com/SignOutOptions. The signed-out landing has a sign-in link instead, so
      // SignOutOptions is the positive, unambiguous "signed in" signal.
      if(document.querySelector('a[href*="SignOutOptions"]')){
        stable++;
        if(stable>=2){ post('LOGIN_OK'); clearInterval(iv); }
      } else {
        stable=0;
      }
    }catch(e){}
  }, 1000);
})();"#;

// Injected on the hidden /usage document. Waits for the usage numbers to render, scrapes them, and
// posts them back over IPC. If Google bounced us to the sign-in page, reports LOGIN instead.
const SCRAPE_JS: &str = r#"(function(){
  function post(m){ try{ window.ipc.postMessage(m); }catch(e){} }
  function iso(s){
    try{
      var now=new Date(), y=now.getFullYear(), mo=now.getMonth(), d=now.getDate();
      var km=s.match(/(\d{1,2})월\s*(\d{1,2})일/);
      if(km){mo=parseInt(km[1])-1;d=parseInt(km[2]);}
      var hh=null,mm=0,pm=false,am=false;
      var kt=s.match(/(오전|오후)\s*(\d{1,2}):(\d{2})/);
      if(kt){pm=kt[1]==='오후';am=kt[1]==='오전';hh=parseInt(kt[2]);mm=parseInt(kt[3]);}
      else{var et=s.match(/(\d{1,2}):(\d{2})\s*(AM|PM)/i);if(et){hh=parseInt(et[1]);mm=parseInt(et[2]);pm=/pm/i.test(et[3]);am=/am/i.test(et[3]);}}
      if(hh===null)return "";
      if(pm&&hh<12)hh+=12; if(am&&hh===12)hh=0;
      var dt=new Date(y,mo,d,hh,mm,0);
      if(!km&&dt.getTime()<now.getTime())dt=new Date(dt.getTime()+86400000);
      return dt.toISOString();
    }catch(e){return "";}
  }
  function scrape(){
    // Redirected to the Google sign-in flow -> not signed in.
    if(/accounts\.google\.com|\/ServiceLogin/.test(location.href)){ post('SCRAPE:LOGIN'); return true; }
    var text=document.body?document.body.innerText:"";
    var lines=text.split('\n').map(function(l){return l.trim();}).filter(Boolean);
    var res=[];
    for(var i=0;i<lines.length;i++){
      var m=lines[i].match(/^(\d+)%\s*(?:사용됨|used)/);
      if(m){
        var reset="";
        for(var j=Math.max(0,i-2);j<Math.min(lines.length,i+3);j++){
          if(/초기화|reset/i.test(lines[j])){reset=lines[j];break;}
        }
        res.push({pct:parseInt(m[1]),reset:reset,resetIso:iso(reset)});
      }
    }
    if(res.length>=1){
      var pm=text.match(/\b(ultra|pro|advanced|free)\b/i);
      var plan=pm?pm[1].charAt(0).toUpperCase()+pm[1].slice(1).toLowerCase():"";
      // Best-effort Google account email from the account button's label (may be absent).
      var email="";
      try{
        var ab=document.querySelector('a[href*="SignOutOptions"]');
        var src=ab?(ab.getAttribute('aria-label')||ab.getAttribute('title')||ab.textContent||""):"";
        var em=src.match(/[\w.+-]+@[\w.-]+\.\w+/);
        if(em)email=em[0];
      }catch(e){}
      post('SCRAPE:'+JSON.stringify({items:res.slice(0,2),plan:plan,email:email}));
      return true;
    }
    // Signed-out landing page: no account button (SignOutOptions), but a sign-in link to the
    // account chooser. Guard on the absence of SignOutOptions so a still-loading SIGNED-IN /usage
    // page (which has the account button but not yet the numbers) is not misreported as signed out.
    if(!document.querySelector('a[href*="SignOutOptions"]') &&
       document.querySelector('a[href*="accounts.google.com"], a[href*="ServiceLogin"]')){
      post('SCRAPE:LOGIN'); return true;
    }
    return false;
  }
  var n=0;var iv=setInterval(function(){n++;if(scrape()){clearInterval(iv);}else if(n>30){post('SCRAPE:TIMEOUT');clearInterval(iv);}},500);
})();"#;

#[derive(Debug)]
enum Msg {
    /// The webview posted a result over IPC.
    Ipc(String),
    /// The overall deadline elapsed.
    Timeout,
}

/// Entry point for a helper process. `mode` is "login" (visible sign-in window) or "scrape"
/// (hidden /usage window). Prints exactly one `SM_RESULT <payload>` line, then exits.
pub fn run(mode: &str) {
    let is_login = mode == "login";
    let udf = std::env::var("SM_GEMINI_UDF").ok().map(PathBuf::from);
    let url = if is_login { HOME_URL } else { USAGE_URL };
    let init_js = if is_login { LOGIN_JS } else { SCRAPE_JS };
    // Login gets a generous window (the user types a password + 2FA); scrape must be quick.
    let timeout = if is_login {
        Duration::from_secs(290)
    } else {
        Duration::from_secs(40)
    };

    let event_loop = EventLoopBuilder::<Msg>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let window = match WindowBuilder::new()
        .with_title("Google")
        .with_visible(is_login)
        .with_inner_size(tao::dpi::LogicalSize::new(920.0, 760.0))
        .build(&event_loop)
    {
        Ok(w) => w,
        Err(e) => {
            emit(&format!("ERROR window {e}"));
            return;
        }
    };

    let mut web_context = WebContext::new(udf);
    let ipc_proxy = proxy.clone();
    let webview = WebViewBuilder::new_with_web_context(&mut web_context)
        .with_url(url)
        .with_user_agent(FIREFOX_UA)
        .with_initialization_script(init_js)
        .with_ipc_handler(move |req: wry::http::Request<String>| {
            let _ = ipc_proxy.send_event(Msg::Ipc(req.into_body()));
        })
        .build(&window);
    if let Err(e) = webview {
        emit(&format!("ERROR webview {e}"));
        return;
    }

    // Deadline watchdog: a separate thread wakes the loop so a wedged page can't hang forever.
    let timeout_proxy = proxy.clone();
    std::thread::spawn(move || {
        std::thread::sleep(timeout);
        let _ = timeout_proxy.send_event(Msg::Timeout);
    });

    event_loop.run(move |event, _target, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::UserEvent(Msg::Ipc(body)) => {
                if is_login {
                    if body == "LOGIN_OK" {
                        emit("LOGIN_OK");
                        *control_flow = ControlFlow::Exit;
                    }
                } else if let Some(rest) = body.strip_prefix("SCRAPE:") {
                    emit(rest);
                    *control_flow = ControlFlow::Exit;
                }
            }
            Event::UserEvent(Msg::Timeout) => {
                emit("TIMEOUT");
                *control_flow = ControlFlow::Exit;
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                emit(if is_login { "CANCELLED" } else { "CLOSED" });
                *control_flow = ControlFlow::Exit;
            }
            _ => {}
        }
    });
}

/// Print the single machine-readable result line and flush (stdout is block-buffered on a pipe).
fn emit(payload: &str) {
    use std::io::Write;
    let mut out = std::io::stdout();
    let _ = writeln!(out, "SM_RESULT {payload}");
    let _ = out.flush();
}
