//! Puertos en uso: qué proceso escucha en cada puerto (para el típico
//! "address already in use"), con la opción de cerrarlo.

use std::process::Command;

#[derive(Clone, Debug, PartialEq)]
pub struct PortInfo {
    pub port: u16,
    pub addr: String,
    pub pid: u32,
    pub name: String,
}

fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let mut c = Command::new(cmd);
    c.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = c.output().ok()?;
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Puertos TCP escuchando, ordenados, sin repetir (puerto, proceso).
pub fn list() -> Vec<PortInfo> {
    let mut v = if cfg!(windows) {
        let names = run("tasklist", &["/FO", "CSV", "/NH"])
            .map(|s| parse_tasklist(&s))
            .unwrap_or_default();
        run("netstat", &["-ano"])
            .map(|s| parse_netstat(&s, &names))
            .unwrap_or_default()
    } else {
        run("ss", &["-ltnpH"])
            .map(|s| parse_ss(&s))
            .filter(|v| !v.is_empty())
            .unwrap_or_else(proc_net)
    };
    v.sort_by_key(|p| (p.port, p.pid));
    v.dedup_by_key(|p| (p.port, p.pid));
    v
}

/// Sin `ss`: lee /proc/net/tcp{,6} (estado 0A = escuchando) y busca qué
/// proceso tiene abierto cada socket.
fn proc_net() -> Vec<PortInfo> {
    let mut socks = vec![];
    for (file, v6) in [("/proc/net/tcp", false), ("/proc/net/tcp6", true)] {
        let Ok(text) = std::fs::read_to_string(file) else { continue };
        for l in text.lines().skip(1) {
            let f: Vec<&str> = l.split_whitespace().collect();
            if f.len() < 10 || f[3] != "0A" {
                continue;
            }
            let Some((_, port)) = f[1].split_once(':') else { continue };
            let Ok(port) = u16::from_str_radix(port, 16) else { continue };
            let addr = if v6 { "[::]" } else { "0.0.0.0" };
            socks.push((port, addr.to_string(), f[9].to_string()));
        }
    }
    // inodo → (pid, nombre)
    let mut owners = std::collections::HashMap::new();
    if let Ok(procs) = std::fs::read_dir("/proc") {
        for p in procs.flatten() {
            let Ok(pid) = p.file_name().to_string_lossy().parse::<u32>() else { continue };
            let Ok(fds) = std::fs::read_dir(p.path().join("fd")) else { continue };
            let name = std::fs::read_to_string(p.path().join("comm"))
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|_| "?".into());
            for fd in fds.flatten() {
                if let Ok(t) = std::fs::read_link(fd.path()) {
                    let t = t.to_string_lossy();
                    if let Some(ino) = t.strip_prefix("socket:[").and_then(|x| x.strip_suffix(']')) {
                        owners.insert(ino.to_string(), (pid, name.clone()));
                    }
                }
            }
        }
    }
    socks
        .into_iter()
        .map(|(port, addr, ino)| {
            let (pid, name) = owners.get(&ino).cloned().unwrap_or((0, "?".into()));
            PortInfo { port, addr, pid, name }
        })
        .collect()
}

/// Cierra el proceso (y sus hijos en Windows).
pub fn kill(pid: u32) -> Result<(), String> {
    let pid_s = pid.to_string();
    let ok = if cfg!(windows) {
        let mut c = Command::new("taskkill");
        c.args(["/PID", &pid_s, "/T", "/F"]);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            c.creation_flags(0x0800_0000);
        }
        c.status()
    } else {
        Command::new("kill").args(["-9", &pid_s]).status()
    };
    match ok {
        Ok(s) if s.success() => Ok(()),
        Ok(_) => Err(format!(
            "No pude cerrar el proceso {pid} (puede ser del sistema o de otro usuario)"
        )),
        Err(e) => Err(e.to_string()),
    }
}

/// `"node.exe","1234","Console","1","50.000 K"` → pid → nombre.
fn parse_tasklist(s: &str) -> std::collections::HashMap<u32, String> {
    s.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split("\",\"").map(|x| x.trim_matches('"')).collect();
            Some((f.get(1)?.parse().ok()?, f.first()?.to_string()))
        })
        .collect()
}

/// `TCP    0.0.0.0:3000    0.0.0.0:0    LISTENING    1234`
fn parse_netstat(s: &str, names: &std::collections::HashMap<u32, String>) -> Vec<PortInfo> {
    s.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            // El estado viene traducido (LISTENING, ESCUCHANDO…): se reconoce
            // por la dirección remota vacía (0.0.0.0:0 o [::]:0).
            if f.len() < 5 || !f[0].starts_with("TCP") || !f[2].ends_with(":0") {
                return None;
            }
            let (addr, port) = f[1].rsplit_once(':')?;
            let pid: u32 = f[4].parse().ok()?;
            Some(PortInfo {
                port: port.parse().ok()?,
                addr: addr.to_string(),
                pid,
                name: names.get(&pid).cloned().unwrap_or_else(|| "?".into()),
            })
        })
        .collect()
}

/// `LISTEN 0 511 0.0.0.0:3000 0.0.0.0:* users:(("node",pid=123,fd=19))`
fn parse_ss(s: &str) -> Vec<PortInfo> {
    s.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            let local = f.get(3)?;
            let (addr, port) = local.rsplit_once(':')?;
            let users = l.split("users:((").nth(1).unwrap_or("");
            let name = users.split('"').nth(1).unwrap_or("?").to_string();
            let pid = users
                .split("pid=")
                .nth(1)
                .and_then(|x| x.split([',', ')']).next())
                .and_then(|x| x.parse().ok())
                .unwrap_or(0);
            Some(PortInfo {
                port: port.parse().ok()?,
                addr: addr.to_string(),
                pid,
                name,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_windows_output() {
        let names = parse_tasklist(
            "\"node.exe\",\"4242\",\"Console\",\"1\",\"80.000 K\"\n\"svchost.exe\",\"900\",\"Services\",\"0\",\"9.000 K\"\n",
        );
        let v = parse_netstat(
            "\nConexiones activas\n\n  Proto  Dirección local  Dirección remota  Estado  PID\n  TCP    0.0.0.0:3000   0.0.0.0:0   LISTENING   4242\n  TCP    [::]:135   [::]:0   LISTENING   900\n  TCP    127.0.0.1:5000   127.0.0.1:6000   ESTABLISHED   4242\n  UDP    0.0.0.0:53   *:*   77\n",
            &names,
        );
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].port, 3000);
        assert_eq!(v[0].name, "node.exe");
        assert_eq!(v[1].addr, "[::]");
        assert_eq!(v[1].port, 135);
    }

    #[test]
    fn parses_ss_output() {
        let v = parse_ss("LISTEN 0 511 0.0.0.0:5173 0.0.0.0:* users:((\"node\",pid=321,fd=19))\nLISTEN 0 4096 [::]:22 [::]:*\n");
        assert_eq!(v[0], PortInfo { port: 5173, addr: "0.0.0.0".into(), pid: 321, name: "node".into() });
        assert_eq!(v[1].port, 22);
        assert_eq!(v[1].pid, 0);
    }
}
