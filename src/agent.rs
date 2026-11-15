//! The browsing agent: an OpenAI-compatible chat loop on a background thread.
//! It only talks HTTP; page work is sent to the UI thread as tool calls.

use crate::config::Config;
use serde_json::{json, Value};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

/// The model is named in the prompt because most endpoints do not know their own id.
fn system_prompt(model: &str) -> String {
    format!(
        "You are the browsing agent inside Scrape, a small desktop browser, \
         running on the model `{model}`. \
         You can read the current page and act on it with the tools you are given. \
         Use the tools instead of guessing page content. Keep answers short and concrete. \
         Never claim an action worked unless a tool result says so."
    )
}

const ROUNDS: usize = 5;

/// What the agent is doing right now, for the strip under the chat.
#[derive(Clone, Copy, PartialEq)]
pub enum Status {
    Idle,
    Thinking,
    Working,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Idle => "idle",
            Status::Thinking => "thinking…",
            Status::Working => "working…",
        }
    }
}

pub struct ToolCall {
    pub name: String,
    pub args: Value,
}

enum Request {
    Ask {
        prompt: String,
        context: String,
        base_url: String,
        model: String,
        api_key: String,
    },
}

enum Evt {
    Text(String),
    Tool(ToolCall),
    /// The model the endpoint actually answered with.
    Model(String),
    /// Prompt tokens of the last request, i.e. how much context is in use.
    Used(u32),
    Done,
    Error(String),
}

pub struct Agent {
    pub msgs: Vec<(bool, String)>,
    pub input: String,
    pub busy: bool,
    pub status: Status,
    pub model: String,
    pub used: u32,
    pub error: Option<String>,
    req: Sender<Request>,
    evt: Receiver<Evt>,
    tool: Sender<Result<String, String>>,
}

impl Agent {
    pub fn new() -> Self {
        let (req, req_rx) = channel();
        let (evt_tx, evt) = channel();
        let (tool, tool_rx) = channel();
        std::thread::spawn(move || worker(req_rx, evt_tx, tool_rx));
        Self {
            msgs: Vec::new(),
            input: String::new(),
            busy: false,
            status: Status::Idle,
            model: String::new(),
            used: 0,
            error: None,
            req,
            evt,
            tool,
        }
    }

    pub fn ask(&mut self, cfg: &Config, context: String) {
        let prompt = self.input.trim().to_owned();
        if prompt.is_empty() || self.busy {
            return;
        }
        self.input.clear();
        self.msgs.push((true, prompt.clone()));
        self.error = None;
        self.busy = true;
        self.status = Status::Thinking;
        if self.model.is_empty() {
            self.model = cfg.agent_model.clone();
        }
        let _ = self.req.send(Request::Ask {
            prompt,
            context,
            base_url: cfg.agent_base_url.clone(),
            model: cfg.agent_model.clone(),
            api_key: cfg.agent_api_key.clone(),
        });
    }

    /// Returns the page actions requested since the last call and whether anything arrived.
    pub fn poll(&mut self) -> (Vec<ToolCall>, bool) {
        let mut tools = Vec::new();
        let mut changed = false;
        while let Ok(evt) = self.evt.try_recv() {
            changed = true;
            match evt {
                Evt::Text(t) => {
                    self.status = Status::Thinking;
                    self.msgs.push((false, t));
                }
                Evt::Tool(c) => {
                    self.status = Status::Working;
                    tools.push(c);
                }
                Evt::Model(m) => self.model = m,
                Evt::Used(n) => self.used = n,
                Evt::Done => {
                    self.busy = false;
                    self.status = Status::Idle;
                }
                Evt::Error(e) => {
                    self.error = Some(e);
                    self.busy = false;
                    self.status = Status::Idle;
                }
            }
        }
        (tools, changed)
    }

    pub fn tool_sender(&self) -> Sender<Result<String, String>> {
        self.tool.clone()
    }

    pub fn tool_done(&mut self, result: Result<String, String>) {
        let _ = self.tool.send(result);
        self.status = Status::Thinking;
    }
}

/// JavaScript for a tool call, or None for tools the UI handles itself (open_url).
pub fn script(call: &ToolCall) -> Option<String> {
    let arg = |k: &str| call.args[k].as_str().unwrap_or("").to_owned();
    let lit = |s: &str| serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into());
    let escaped = |s: &str| lit(s).replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029");
    Some(match call.name.as_str() {
        "read_page" => "JSON.stringify({title: document.title, url: location.href, \
             text: (document.body ? document.body.innerText : '').slice(0, 6000)})"
            .into(),
        "click" => format!(
            "(()=>{{const q={};let el=document.querySelector(q);\
             if(!el){{const t=q.toLowerCase();el=[...document.querySelectorAll('a,button,[role=button],input,summary,label')]\
             .find(e=>((e.innerText||e.value||'')+'').trim().toLowerCase().includes(t));}}\
             if(!el)return JSON.stringify('nothing matched '+q);\
             el.scrollIntoView({{block:'center'}});el.click();\
             return JSON.stringify('clicked '+((el.innerText||el.value||el.tagName)+'').trim().slice(0,80));}})()",
            escaped(&arg("target"))
        ),
        "fill" => format!(
            "(()=>{{const sel={};const el=document.querySelector(sel);\
             if(!el)return JSON.stringify('no element for '+sel);\
             el.focus();el.value={};\
             el.dispatchEvent(new Event('input',{{bubbles:true}}));\
             el.dispatchEvent(new Event('change',{{bubbles:true}}));\
             return JSON.stringify('filled '+sel);}})()",
            escaped(&arg("selector")),
            escaped(&arg("text"))
        ),
        _ => return None,
    })
}

/// WebView2 returns the script result JSON-encoded; the tools already return strings.
pub fn unwrap_result(raw: String) -> String {
    serde_json::from_str::<String>(&raw).unwrap_or(raw)
}

fn tools() -> Value {
    json!([
        {"type": "function", "function": {
            "name": "read_page",
            "description": "Read the title, url and visible text of the current page.",
            "parameters": {"type": "object", "properties": {}},
        }},
        {"type": "function", "function": {
            "name": "open_url",
            "description": "Navigate the current tab to a url.",
            "parameters": {
                "type": "object",
                "properties": {"url": {"type": "string"}},
                "required": ["url"],
            },
        }},
        {"type": "function", "function": {
            "name": "click",
            "description": "Click an element by CSS selector, or by its visible text if no selector is given.",
            "parameters": {
                "type": "object",
                "properties": {"target": {"type": "string", "description": "CSS selector or visible text"}},
                "required": ["target"],
            },
        }},
        {"type": "function", "function": {
            "name": "fill",
            "description": "Type text into an input matching a CSS selector.",
            "parameters": {
                "type": "object",
                "properties": {
                    "selector": {"type": "string"},
                    "text": {"type": "string"},
                },
                "required": ["selector", "text"],
            },
        }},
    ])
}

fn worker(req_rx: Receiver<Request>, evt: Sender<Evt>, tool_rx: Receiver<Result<String, String>>) {
    let http = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(120)))
        .build()
        .new_agent();
    // Conversation state lives here so the UI stays a plain renderer.
    let mut history: Vec<Value> = Vec::new();

    while let Ok(Request::Ask {
        prompt,
        context,
        base_url,
        model,
        api_key,
    }) = req_rx.recv()
    {
        history.push(json!({
            "role": "user",
            "content": format!("{prompt}\n\nPage context:\n{context}"),
        }));

        for _ in 0..ROUNDS {
            let messages: Vec<Value> =
                std::iter::once(json!({"role": "system", "content": system_prompt(&model)}))
                    .chain(history.iter().cloned())
                    .collect();
            // Endpoints without a usage block still get a rough fill for the context strip.
            let estimate = serde_json::to_string(&messages)
                .map(|s| (s.len() / 4) as u32)
                .unwrap_or(0);
            let body = json!({
                "model": model,
                "messages": messages,
                "tools": tools(),
                "tool_choice": "auto",
            });
            let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
            let message = match http
                .post(&url)
                .header("Authorization", format!("Bearer {api_key}"))
                .send_json(&body)
            {
                Ok(mut resp) => match resp.body_mut().read_json::<Value>() {
                    Ok(mut v) => {
                        if let Some(answered) = v["model"].as_str() {
                            evt.send(Evt::Model(answered.to_owned())).ok();
                        }
                        let used = v["usage"]["prompt_tokens"]
                            .as_u64()
                            .map(|n| n as u32)
                            .unwrap_or(estimate);
                        evt.send(Evt::Used(used)).ok();
                        let choice = v["choices"][0]["message"].take();
                        if choice.is_null() {
                            evt.send(Evt::Error(
                                "The endpoint answered without choices[0].message.".into(),
                            ))
                            .ok();
                            break;
                        }
                        choice
                    }
                    Err(e) => {
                        evt.send(Evt::Error(format!("Unreadable response: {e}"))).ok();
                        break;
                    }
                },
                Err(e) => {
                    evt.send(Evt::Error(format!(
                        "{e}\nCheck the base URL, model and key in scrapeconfig."
                    )))
                    .ok();
                    break;
                }
            };

            let content = message["content"].as_str().unwrap_or("").trim().to_owned();
            if !content.is_empty() {
                evt.send(Evt::Text(content.clone())).ok();
            }
            let calls = message["tool_calls"].as_array().cloned().unwrap_or_default();
            if calls.is_empty() {
                history.push(json!({"role": "assistant", "content": content}));
                break;
            }

            history.push(message);
            for call in calls {
                let id = call["id"].as_str().unwrap_or("call").to_owned();
                let name = call["function"]["name"].as_str().unwrap_or("").to_owned();
                let args = serde_json::from_str::<Value>(
                    call["function"]["arguments"].as_str().unwrap_or("{}"),
                )
                .unwrap_or_else(|_| json!({}));
                evt.send(Evt::Tool(ToolCall { name, args })).ok();
                let result = match tool_rx.recv_timeout(Duration::from_secs(30)) {
                    Ok(Ok(text)) => text,
                    Ok(Err(e)) => format!("error: {e}"),
                    Err(RecvTimeoutError::Timeout) => "error: the page did not answer in time".into(),
                    Err(RecvTimeoutError::Disconnected) => "error: browser is shutting down".into(),
                };
                history.push(json!({
                    "role": "tool",
                    "tool_call_id": id,
                    "content": result,
                }));
            }
        }

        evt.send(Evt::Done).ok();
    }
}
