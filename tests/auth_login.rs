// Copyright 2026 Google LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Google Sign-In (`/auth/login` → Google → `/oauth2/callback`) over real HTTP
//! with Google's token and userinfo endpoints mocked:
//!
//! - the callback only completes a sign-in in the browser that started it
//!   (nonce cookie), defeating login CSRF;
//! - accounts whose email address is not verified are refused;
//! - a verified account gets the signed `adspace_user` cookie.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use adk_rust::session::InMemorySessionService;
use adspace_agent::auth::{GoogleOAuthFlow, OAuthClientConfig, SessionCredentialManager};
use adspace_agent::server::{AdminAllowList, OAuthRoutes, USER_COOKIE_NAME, oauth_router};
use axum::Router;
use axum::extract::State;
use axum::routing::{get, post};
use serde_json::{Value, json};

async fn mock_token_endpoint() -> axum::Json<Value> {
    axum::Json(json!({
        "access_token": "ya29.login-token",
        "expires_in": 3600,
        "scope": "openid email profile",
        "token_type": "Bearer",
    }))
}

async fn mock_userinfo(State(verified): State<Arc<AtomicBool>>) -> axum::Json<Value> {
    axum::Json(json!({
        "sub": "1122334455",
        "email": "nate@example.com",
        "email_verified": verified.load(Ordering::Relaxed),
        "name": "Nate",
        "picture": "https://example.com/nate.png",
    }))
}

async fn spawn(router: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(axum::serve(listener, router).into_future());
    format!("http://{addr}")
}

fn cookie_pair(set_cookie: &str) -> String {
    set_cookie.split(';').next().unwrap().trim().to_string()
}

#[tokio::test]
async fn sign_in_is_bound_to_the_browser_and_requires_a_verified_email() {
    let verified = Arc::new(AtomicBool::new(false));
    let google = spawn(
        Router::new()
            .route("/token", post(mock_token_endpoint))
            .route("/userinfo", get(mock_userinfo))
            .with_state(verified.clone()),
    )
    .await;
    let flow = GoogleOAuthFlow::new(OAuthClientConfig {
        client_id: "cid".into(),
        client_secret: "secret".into(),
        redirect_uri: "http://localhost:8000/oauth2/callback".into(),
    })
    .with_endpoints(format!("{google}/auth"), format!("{google}/token"))
    .with_userinfo_endpoint(format!("{google}/userinfo"));
    let credentials = Arc::new(SessionCredentialManager::new(flow));
    let agent = spawn(oauth_router(OAuthRoutes {
        credentials: credentials.clone(),
        sessions: Arc::new(InMemorySessionService::new()),
        resume: None,
        admins: AdminAllowList::default(),
    }))
    .await;
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();

    // 1. `/auth/login` redirects to Google and plants the nonce cookie.
    let resp = http
        .get(format!("{agent}/auth/login"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::SEE_OTHER);
    let location = resp.headers()[reqwest::header::LOCATION]
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        location.starts_with(&format!("{google}/auth?")),
        "{location}"
    );
    let nonce_cookie = cookie_pair(
        resp.headers()[reqwest::header::SET_COOKIE]
            .to_str()
            .unwrap(),
    );
    assert!(nonce_cookie.starts_with("adspace_login="), "{nonce_cookie}");
    let state = url::Url::parse(&location)
        .unwrap()
        .query_pairs()
        .find(|(k, _)| k == "state")
        .map(|(_, v)| v.to_string())
        .unwrap();
    let callback = format!("{agent}/oauth2/callback?code=the-code&state={state}");

    // 2. A victim's browser (no nonce cookie) cannot be signed in with this state.
    let resp = http.get(&callback).send().await.unwrap();
    assert!(resp.headers().get(reqwest::header::SET_COOKIE).is_none());
    let page = resp.text().await.unwrap();
    assert!(page.contains("not started in this browser"), "{page}");

    // 3. The initiating browser is refused while the account's email is unverified.
    let resp = http
        .get(&callback)
        .header(reqwest::header::COOKIE, &nonce_cookie)
        .send()
        .await
        .unwrap();
    let set_cookies: Vec<String> = resp
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .map(|v| v.to_str().unwrap().to_string())
        .collect();
    assert!(
        !set_cookies
            .iter()
            .any(|c| c.starts_with(&format!("{USER_COOKIE_NAME}="))),
        "{set_cookies:?}"
    );
    let page = resp.text().await.unwrap();
    assert!(page.contains("not verified"), "{page}");

    // 4. With a verified email the sign-in completes: the user cookie is set
    //    and the nonce cookie is cleared.
    verified.store(true, Ordering::Relaxed);
    let resp = http
        .get(&callback)
        .header(reqwest::header::COOKIE, &nonce_cookie)
        .send()
        .await
        .unwrap();
    let set_cookies: Vec<String> = resp
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .map(|v| v.to_str().unwrap().to_string())
        .collect();
    let user_cookie = set_cookies
        .iter()
        .find(|c| c.starts_with(&format!("{USER_COOKIE_NAME}=")))
        .unwrap_or_else(|| panic!("user cookie set: {set_cookies:?}"));
    assert!(user_cookie.contains("HttpOnly") && user_cookie.contains("SameSite=Lax"));
    assert!(
        !user_cookie.contains("Secure"),
        "plain http base URL: {user_cookie}"
    );
    assert!(
        set_cookies
            .iter()
            .any(|c| c.starts_with("adspace_login=;") && c.contains("Max-Age=0")),
        "{set_cookies:?}"
    );
    let page = resp.text().await.unwrap();
    assert!(
        page.contains("Signed in as <strong>Nate</strong>"),
        "{page}"
    );

    let user = credentials
        .verify_user_cookie(cookie_pair(user_cookie).trim_start_matches("adspace_user="))
        .expect("cookie verifies");
    // The user id is the Google `sub`, never the (changeable) email address.
    assert_eq!(user.id, "1122334455");
    assert_eq!(user.email, "nate@example.com");
    assert_eq!(user.sub.as_deref(), Some("1122334455"));

    // 5. `/auth/me` recognises the cookie.
    let me: Value = http
        .get(format!("{agent}/auth/me"))
        .header(reqwest::header::COOKIE, cookie_pair(user_cookie))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(me["authenticated"], true);
    assert_eq!(me["user"]["email"], "nate@example.com");
    assert_eq!(me["is_admin"], false, "no ADMIN_EMAILS configured");
}
