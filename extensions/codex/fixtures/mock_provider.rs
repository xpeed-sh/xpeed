//! Test fixture, excluded from production builds and extension packages.
use serde_json::{Value, json};
use std::io::{self, BufRead, Write};
fn emit(v: Value) {
    println!("{v}");
    io::stdout().flush().unwrap();
}
fn event(method: &str, params: Value) {
    emit(json!({"method":method,"params":params}));
}
fn main() {
    if std::env::args().any(|arg| arg == "--version") {
        println!("codex-cli 0.162.0-alpha.2");
        return;
    }
    let mut logged_in = false;
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else {
            break;
        };
        let v: Value = serde_json::from_str(&line).unwrap();
        let id = v["id"].clone();
        let result = match v["method"].as_str() {
            Some("initialize") => json!({"userAgent":"test"}),
            Some("initialized") => continue,
            Some("account/read") => {
                json!({"account":if logged_in {json!({"type":"chatgpt","email":"test@example.invalid","planType":"test"})}else{Value::Null},"requiresOpenaiAuth":true})
            }
            Some("model/list") => {
                json!({"data":[{"model":"mock-model","displayName":"Mock","hidden":false}]})
            }
            Some("account/login/start") => {
                emit(
                    json!({"id":id,"result":{"loginId":"mock-login","authUrl":"https://example.invalid/login","type":"chatgpt"}}),
                );
                logged_in = true;
                event(
                    "account/login/completed",
                    json!({"loginId":"mock-login","success":true,"error":null}),
                );
                continue;
            }
            Some("account/login/cancel") => json!({"status":"canceled"}),
            Some("account/logout") => {
                logged_in = false;
                json!({})
            }
            Some("thread/start") => {
                assert_eq!(v["params"]["sandbox"], "read-only");
                assert_eq!(v["params"]["approvalPolicy"], "never");
                assert_eq!(v["params"]["environments"], json!([]));
                assert_eq!(v["params"]["dynamicTools"].as_array().unwrap().len(), 9);
                json!({"thread":{"id":"mock-thread"}})
            }
            Some("thread/resume") => {
                assert_eq!(v["params"]["threadId"], "mock-thread");
                assert!(
                    v["params"]["developerInstructions"]
                        .as_str()
                        .unwrap()
                        .contains("resumed-scope")
                );
                assert_eq!(v["params"]["sandbox"], "read-only");
                assert_eq!(v["params"]["approvalPolicy"], "never");
                json!({"thread":{"id":"mock-thread"}})
            }
            Some("turn/start") => {
                if v["params"]["input"][0]["text"] == "fail" {
                    emit(json!({"id":id,"error":{"code":-32000,"message":"Mock failure"}}));
                    continue;
                }
                emit(json!({"id":id,"result":{"turn":{"id":"mock-turn"}}}));
                event("turn/started", json!({"turn":{"id":"mock-turn"}}));
                if v["params"]["input"][0]["text"] == "interrupt" {
                    continue;
                }
                emit(
                    json!({"id":700,"method":"item/commandExecution/requestApproval","params":{"command":"echo mock","reason":"Test"}}),
                );
                continue;
            }
            Some("turn/interrupt") => {
                emit(json!({"id":id,"result":{}}));
                event(
                    "turn/completed",
                    json!({"turn":{"id":"mock-turn","status":"interrupted"}}),
                );
                continue;
            }
            None if id == 700 => {
                assert_eq!(v["result"]["decision"], "decline");
                emit(
                    json!({"id":701,"method":"item/tool/call","params":{"threadId":"mock-thread","turnId":"mock-turn","callId":"fixture-tool","tool":"xpeed_list_sessions","arguments":{}}}),
                );
                continue;
            }
            None if id == 701 => {
                assert_eq!(v["result"]["success"], true);
                event("serverRequest/resolved", json!({"requestId":700}));
                event(
                    "item/agentMessage/delta",
                    json!({"itemId":"msg","delta":"Hola "}),
                );
                event(
                    "item/agentMessage/delta",
                    json!({"itemId":"msg","delta":"mundo"}),
                );
                event(
                    "item/completed",
                    json!({"item":{"id":"msg","type":"agentMessage","text":"Hola mundo"}}),
                );
                event(
                    "turn/completed",
                    json!({"turn":{"id":"mock-turn","status":"completed","error":null}}),
                );
                continue;
            }
            _ => panic!("Unexpected fixture method"),
        };
        emit(json!({"id":id,"result":result}));
    }
}
