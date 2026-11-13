//! The image tools: `generate_image` has OpenAI's image API draw a PNG into
//! the project, for UI art and the like that an agent cannot draw itself,
//! and `image_models` lists the image models the key reaches, so an agent
//! can move to a newer one without a new Besfa.
//!
//! curl, which Windows ships, makes the requests, so the server needs no
//! HTTP client of its own. The key goes to curl on stdin, never in its
//! arguments, where other processes could read it.

use std::fs;
use std::io::Write;
use std::path::{Component, Path};
use std::process::Stdio;

use base64::Engine;
use serde_json::{Value, json};

use crate::quiet;

/// The model asked for when the call names none: the newest the key
/// reached on 2026-11-13 that draws transparent backgrounds, which UI
/// pieces need (gpt-image-2 refuses them). ponytail: a constant;
/// `image_models` and `model` reach newer ones until this one retires.
pub const MODEL: &str = "gpt-image-2.6-flare";

const API: &str = "https://api.openai.com/v1";

/// What the tools take, for `tools/list`.
pub fn tools() -> [Value; 2] {
    [
        json!({
            "name": "generate_image",
            "description": format!("Draws an image with OpenAI's image API (model {MODEL} unless `model` says otherwise) and saves it as a PNG inside the project, for UI art, icons and textures. Needs OPENAI_API_KEY in the server's environment. Each call costs money on the user's OpenAI account, billed by the tokens it uses, which grow with quality and size, so draw what you need at the quality you need, and look at the file before drawing it again. Returns the saved path and the tokens it used."),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "prompt": {"type": "string", "description": "What to draw: subject, style, colors, framing."},
                    "file": {"type": "string", "description": "Where to save it, a .png relative to the project, as assets/ui/button.png. Folders are made; an existing file is replaced."},
                    "size": {"type": "string", "description": "1024x1024 (default), 1536x1024, 1024x1536 or auto."},
                    "quality": {"type": "string", "description": "low, medium, high or auto (default). Low is cheapest, for drafts."},
                    "background": {"type": "string", "description": "transparent for UI pieces and icons, opaque, or auto (default). Some models refuse transparent."},
                    "model": {"type": "string", "description": format!("An OpenAI image model, {MODEL} by default; `image_models` lists them.")},
                    "project": {"type": "string", "description": "Absolute path of the game project; by default the folder the server runs in."},
                },
                "required": ["prompt", "file"],
            },
        }),
        json!({
            "name": "image_models",
            "description": format!("Lists the OpenAI image models OPENAI_API_KEY can use, sorted by name, for `generate_image`'s `model`. {MODEL} is its default. Free."),
            "inputSchema": {"type": "object", "properties": {}},
        }),
    ]
}

/// Draws what `args` asks for into the project at `project`.
pub fn generate(project: &Path, args: &Value) -> Result<String, String> {
    let prompt = args["prompt"].as_str().ok_or("Give `prompt`.")?;
    let file = args["file"].as_str().ok_or("Give `file`, a .png path.")?;
    if !file.to_lowercase().ends_with(".png") {
        return Err("`file` must end in .png.".into());
    }
    let relative = Path::new(file);
    if relative.is_absolute()
        || relative.components().any(|part| {
            matches!(
                part,
                Component::ParentDir | Component::Prefix(_) | Component::RootDir
            )
        })
    {
        return Err(format!("`file` must stay inside the project: {file}"));
    }
    // A colon would put the image in an NTFS stream nothing opens.
    if file.contains([':', '<', '>', '"', '|', '?', '*']) {
        return Err(format!(
            "`file` holds a character file names cannot: {file}"
        ));
    }
    // Before the request, which costs money, rather than after it.
    let path = project.join(relative);
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder)
            .map_err(|error| format!("Could not make the folder for {file}: {error}"))?;
    }
    if path.is_dir() {
        return Err(format!("{file} is a folder."));
    }
    let key = key()?;
    let mut body = json!({
        "model": args["model"].as_str().unwrap_or(MODEL),
        "prompt": prompt,
        "n": 1,
    });
    for name in ["size", "quality", "background"] {
        if let Some(value) = args[name].as_str() {
            body[name] = json!(value);
        }
    }
    let response = request(&key, "images/generations", Some(&body))?;
    let image = response["data"][0]["b64_json"]
        .as_str()
        .ok_or_else(|| format!("OpenAI answered without an image: {response}"))?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(image)
        .map_err(|error| format!("OpenAI's image is not base64: {error}"))?;
    fs::write(&path, bytes).map_err(|error| format!("Could not write {file}: {error}"))?;
    Ok(format!(
        "Saved {}. Usage: {}",
        file.replace('\\', "/"),
        response["usage"]
    ))
}

/// The image models the key reaches, one a line, sorted.
pub fn models() -> Result<String, String> {
    let response = request(&key()?, "models", None)?;
    let mut names: Vec<&str> = response["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|model| model["id"].as_str())
        // DALL-E answers with a URL unless asked otherwise, so it is left out.
        .filter(|id| id.contains("image"))
        .collect();
    names.sort_unstable();
    if names.is_empty() {
        return Ok("OPENAI_API_KEY reaches no image model.".into());
    }
    Ok(names.join("\n"))
}

fn key() -> Result<String, String> {
    std::env::var("OPENAI_API_KEY")
        .ok()
        .filter(|key| !key.trim().is_empty())
        .ok_or("OPENAI_API_KEY is not set where the MCP server runs. Set it as a user or system environment variable and restart the agent.".into())
}

/// Calls OpenAI's `endpoint`, posting `body` if there is one, and reads its
/// JSON answer; an error answer is an Err with OpenAI's message.
fn request(key: &str, endpoint: &str, body: Option<&Value>) -> Result<Value, String> {
    let mut curl = quiet("curl");
    curl.args(["-sS", "--max-time", "300", "-H", "@-"]);
    // The prompt can be long and full of quotes, so it goes in a file
    // rather than through Windows' command line quoting.
    let file = std::env::temp_dir().join(format!("besfa-image-{}.json", std::process::id()));
    if let Some(body) = body {
        fs::write(&file, body.to_string()).map_err(|error| error.to_string())?;
        curl.args(["-H", "Content-Type: application/json", "--data-binary"])
            .arg(format!("@{}", file.display()));
    }
    let output = curl
        .arg(format!("{API}/{endpoint}"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            // Headers from stdin, one a line.
            let mut stdin = child.stdin.take().expect("piped");
            writeln!(stdin, "Authorization: Bearer {key}")?;
            drop(stdin);
            child.wait_with_output()
        });
    if body.is_some() {
        let _ = fs::remove_file(&file);
    }
    let output = output.map_err(|error| format!("Could not run curl: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "curl failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let answer: Value = serde_json::from_slice(&output.stdout).map_err(|error| {
        let text = String::from_utf8_lossy(&output.stdout);
        format!(
            "OpenAI's answer is not JSON ({error}): {}",
            text.chars().take(300).collect::<String>()
        )
    })?;
    match answer["error"]["message"].as_str() {
        // That message shows part of the key.
        Some(message) if message.starts_with("Incorrect API key") => {
            Err("OpenAI refused OPENAI_API_KEY as incorrect.".into())
        }
        Some(message) => Err(format!("OpenAI refused it: {message}")),
        None => Ok(answer),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_paths_outside_the_project_before_any_request() {
        let project = std::env::temp_dir();
        for file in [
            "../x.png",
            "C:/x.png",
            "C:x.png",
            "a/../../x.png",
            "/x.png",
            "\\x.png",
            "a.png:s.png",
            "a?.png",
        ] {
            let error = generate(&project, &json!({"prompt": "p", "file": file})).unwrap_err();
            assert!(
                error.contains("inside the project") || error.contains("file names cannot"),
                "{file}: {error}"
            );
        }
        let error = generate(&project, &json!({"prompt": "p", "file": "x.jpg"})).unwrap_err();
        assert!(error.contains(".png"), "{error}");
    }
}
