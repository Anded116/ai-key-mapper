//! Миничат: запускает `claude -p` в корне репозитория и пересылает его поток событий в окно
//! событием `agent`. Следующие сообщения продолжают ту же сессию через `--resume`.

use serde::Serialize;
use serde_json::Value;
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::thread;
use tauri::{AppHandle, Emitter, Manager, State};

/// Всё, что агенту можно без вопросов: окно не умеет отвечать на запросы разрешений, остальное
/// отклоняется (`dontAsk`). Пишет он только профили, запускает только утилиты проекта.
const ALLOWED_TOOLS: &[&str] = &[
    "Read",
    "Glob",
    "Grep",
    "WebSearch",
    "WebFetch",
    "Skill",
    "TodoWrite",
    "TaskStop",
    "Edit(profiles/**)",
    "Bash(target/release/aikm.exe check *)",
    "Bash(target/release/aikm.exe run * --dry)",
    "Bash(target/release/aikm-import *)",
    "Bash(target/release/testrange.exe --screenshot *)",
    "Bash(cargo build --release *)",
    "Bash(reg query *)",
    "Bash(rm profiles/_verify.toml)",
];

const SYSTEM_NOTE: &str = "Ты агент в окне ai-key-mapper: пользователь пишет из миничата и не видит запросов \
разрешений. Файлы можно менять только в profiles/. Команды — только из корня репозитория и ровно в таком виде: \
target/release/aikm.exe check …, target/release/aikm.exe run … --dry, target/release/aikm-import …, \
target/release/testrange.exe --screenshot …, cargo build --release …, reg query …, rm profiles/_verify.toml. \
Остальное будет отклонено; если без этого не обойтись — скажи пользователю, что сделать самому.";

#[derive(Default)]
pub struct Agent {
    session: Mutex<Option<String>>,
    busy: Mutex<bool>,
}

/// Строка ленты чата.
#[derive(Serialize, Clone)]
pub struct AgentEvent {
    /// `text` — ответ агента, `tool` — вызов инструмента, `result` — итог хода, `error`, `done` — процесс завершился.
    pub kind: &'static str,
    pub text: String,
}

/// Корень репозитория: app/src-tauri → ../..
fn repo_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.parent().and_then(|p| p.parent()).map(PathBuf::from).unwrap_or(manifest)
}

#[tauri::command]
pub fn send_chat(text: String, app: AppHandle, agent: State<'_, Agent>) -> Result<(), String> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("пустое сообщение".into());
    }
    {
        let mut busy = agent.busy.lock().unwrap();
        if *busy {
            return Err("агент ещё отвечает на прошлое сообщение".into());
        }
        *busy = true;
    }
    let resume = agent.session.lock().unwrap().clone();
    let mut args = vec!["-p".to_string(), text, "--output-format".into(), "stream-json".into(), "--verbose".into()];
    if let Some(id) = &resume {
        args.push("--resume".into());
        args.push(id.clone());
    }
    // Пользовательские настройки Claude Code разрешают всё подряд — агенту окна они не нужны.
    args.extend(
        ["--setting-sources", "project", "--strict-mcp-config", "--permission-mode", "dontAsk"].map(String::from),
    );
    args.extend(["--append-system-prompt".to_string(), SYSTEM_NOTE.to_string()]);
    args.push("--allowedTools".into());
    args.extend(ALLOWED_TOOLS.iter().map(|t| t.to_string()));

    let mut cmd = Command::new("claude");
    cmd.args(&args)
        .current_dir(repo_root())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Окно запущено из сессии Claude Code (при отладке) — вложенный агент не должен считать себя её частью.
        .env_remove("CLAUDECODE")
        .env_remove("CLAUDE_CODE_ENTRYPOINT");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            *agent.busy.lock().unwrap() = false;
            return Err(format!("не удалось запустить claude: {e}"));
        }
    };

    let stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let err_text = thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        s
    });

    thread::spawn(move || {
        let emit = |kind: &'static str, text: String| {
            let _ = app.emit("agent", AgentEvent { kind, text });
        };
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            let Ok(event) = serde_json::from_str::<Value>(&line) else { continue };
            if let Some(id) = event.get("session_id").and_then(Value::as_str) {
                let state = app.state::<Agent>();
                *state.session.lock().unwrap() = Some(id.to_string());
            }
            for (kind, text) in describe(&event) {
                emit(kind, text);
            }
        }
        let code = child.wait().ok().and_then(|s| s.code());
        let stderr = err_text.join().unwrap_or_default();
        if code != Some(0) {
            let tail: String = stderr.trim().chars().rev().take(600).collect::<Vec<_>>().into_iter().rev().collect();
            emit("error", format!("claude завершился с кодом {}{}", code.map_or("?".into(), |c| c.to_string()),
                if tail.is_empty() { String::new() } else { format!(": {tail}") }));
        }
        *app.state::<Agent>().busy.lock().unwrap() = false;
        emit("done", String::new());
    });
    Ok(())
}

/// Начать новый диалог: следующее сообщение запустит свежую сессию.
#[tauri::command]
pub fn reset_chat(agent: State<'_, Agent>) {
    *agent.session.lock().unwrap() = None;
}

/// Читаемые строки из одного события stream-json.
fn describe(event: &Value) -> Vec<(&'static str, String)> {
    match event.get("type").and_then(Value::as_str) {
        Some("assistant") => event
            .pointer("/message/content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|c| match c.get("type").and_then(Value::as_str) {
                Some("text") => {
                    let t = c.get("text").and_then(Value::as_str).unwrap_or("").trim();
                    (!t.is_empty()).then(|| ("text", t.to_string()))
                }
                Some("tool_use") => {
                    let name = c.get("name").and_then(Value::as_str).unwrap_or("?");
                    let input = c.get("input");
                    let target = ["file_path", "pattern", "path", "command", "description"]
                        .iter()
                        .find_map(|k| input.and_then(|i| i.get(*k)).and_then(Value::as_str))
                        .unwrap_or("");
                    let target: String = target.chars().take(140).collect();
                    Some(("tool", format!("{name} {target}").trim_end().to_string()))
                }
                _ => None,
            })
            .collect(),
        Some("result") => {
            let secs = event.get("duration_ms").and_then(Value::as_f64).unwrap_or(0.0) / 1000.0;
            let cost = event.get("total_cost_usd").and_then(Value::as_f64);
            let failed = event.get("is_error").and_then(Value::as_bool).unwrap_or(false);
            let mut text = format!("{} за {secs:.1} с", if failed { "ошибка" } else { "готово" });
            if let Some(c) = cost {
                text.push_str(&format!(" · ${c:.3}"));
            }
            vec![(if failed { "error" } else { "result" }, text)]
        }
        _ => Vec::new(),
    }
}
