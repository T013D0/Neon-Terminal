//! Servidores de `~/.ssh/config` para abrirlos desde la paleta.

/// Un `Host` del config (sin comodines) con a dónde apunta, si se sabe.
#[derive(Clone, Debug, PartialEq)]
pub struct SshHost {
    pub name: String,
    pub target: Option<String>,
}

pub fn hosts() -> Vec<SshHost> {
    let Some(path) = dirs::home_dir().map(|h| h.join(".ssh").join("config")) else {
        return vec![];
    };
    std::fs::read_to_string(path).map(|t| parse(&t)).unwrap_or_default()
}

pub fn parse(text: &str) -> Vec<SshHost> {
    let mut out: Vec<SshHost> = vec![];
    // Hosts del bloque actual (un `Host` puede nombrar varios).
    let mut current: Vec<usize> = vec![];
    let mut user: Option<String> = None;
    let mut host: Option<String> = None;
    let flush = |out: &mut Vec<SshHost>, current: &[usize], user: &Option<String>, host: &Option<String>| {
        for &i in current {
            out[i].target = match (user, host) {
                (Some(u), Some(h)) => Some(format!("{u}@{h}")),
                (None, Some(h)) => Some(h.clone()),
                _ => None,
            };
        }
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, val) = match line.split_once(|c: char| c.is_whitespace() || c == '=') {
            Some((k, v)) => (k.to_ascii_lowercase(), v.trim().trim_start_matches('=').trim()),
            None => continue,
        };
        match key.as_str() {
            "host" => {
                flush(&mut out, &current, &user, &host);
                current.clear();
                user = None;
                host = None;
                for name in val.split_whitespace() {
                    // Los patrones (*, ?, !) no son servidores concretos.
                    if !name.contains(['*', '?', '!']) && !out.iter().any(|h| h.name == name) {
                        current.push(out.len());
                        out.push(SshHost { name: name.to_string(), target: None });
                    }
                }
            }
            "match" => {
                flush(&mut out, &current, &user, &host);
                current.clear();
            }
            "hostname" => host = Some(val.to_string()),
            "user" => user = Some(val.to_string()),
            _ => {}
        }
    }
    flush(&mut out, &current, &user, &host);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_config() {
        let v = parse(
            "# comentario\nHost *\n  ServerAliveInterval 30\n\nHost prod web\n  HostName 10.0.0.5\n  User deploy\n\nHost=casa\n  HostName=raspberrypi.local\nHost git-*\n  User git\n",
        );
        assert_eq!(
            v,
            vec![
                SshHost { name: "prod".into(), target: Some("deploy@10.0.0.5".into()) },
                SshHost { name: "web".into(), target: Some("deploy@10.0.0.5".into()) },
                SshHost { name: "casa".into(), target: Some("raspberrypi.local".into()) },
            ]
        );
    }
}
