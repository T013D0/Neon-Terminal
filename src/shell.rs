//! Integración con la shell.
//!
//! La terminal no "ve" qué comando ejecutaste: solo recibe bytes. Para saberlo,
//! cada sesión arranca la shell con un pequeño script que, cada vez que se
//! dibuja el prompt, agrega una línea JSON a un archivo de eventos propio de
//! esa sesión (comando, carpeta, si salió bien, duración). La app lee ese
//! archivo de forma incremental.

use egui_term::BackendSettings;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub fn data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("neon")
}

/// Si existe la carpeta de datos del nombre anterior ("warpito"), la renombra
/// para no perder el historial.
pub fn migrate_old_data() {
    let Some(base) = dirs::data_local_dir() else {
        return;
    };
    let (old, new) = (base.join("warpito"), base.join("neon"));
    if old.is_dir() && !new.exists() {
        let _ = std::fs::rename(&old, &new);
    }
}

const PWSH_HOOK: &str = r#"
# Generado por Neon: colores, prompt y reporte de cada comando a la terminal.
$global:__wLog = $env:NEON_EVENTS
$global:__wLastId = -1
$global:__wPrevCwd = (Get-Location).ProviderPath
$global:__wOrigPrompt = $function:prompt
$global:__wFirst = $true
$global:__wHasGit = [bool](Get-Command git -ErrorAction SilentlyContinue)
# Solo usamos nuestro prompt si tienes el de fábrica (respeta oh-my-posh, starship, etc.).
$global:__wOwnPrompt = ($env:NEON_PROMPT -ne '0') -and -not ($env:POSH_THEME -or $env:STARSHIP_SHELL) -and ("$function:prompt" -match 'PS \$\(')

if (Get-Module -ListAvailable PSReadLine) {
    $e = [char]27
    # Colores de la línea mientras escribes.
    try {
        Set-PSReadLineOption -Colors @{
            Command   = "$e[38;2;{{RED}}m"
            Parameter = "$e[38;2;{{CYAN}}m"
            String    = "$e[38;2;{{GREEN}}m"
            Variable  = "$e[38;2;{{VIOLET}}m"
            Number    = "$e[38;2;{{ORANGE}}m"
            Operator  = "$e[38;2;{{MUTED}}m"
            Comment   = "$e[38;2;{{MUTED}}m"
            Keyword   = "$e[38;2;{{VIOLET}}m"
            Type      = "$e[38;2;{{YELLOW}}m"
            Member    = "$e[38;2;{{BLUE}}m"
            Default   = "$e[38;2;{{TEXT}}m"
        }
    } catch {}
    try { Set-PSReadLineOption -Colors @{ InlinePrediction = "$e[38;2;{{MUTED}}m" } } catch {}
    # Sugerencias en gris mientras escribes (→ para aceptar).
    try { Set-PSReadLineOption -PredictionSource History -PredictionViewStyle InlineView } catch {}
}

# Cápsula redondeada: bordes con los glifos  y  (Nerd Font incluida en Neon).
function global:__wPill([string]$bg, [string]$body) {
    $e = [char]27
    return "$e[38;2;${bg}m$([char]0xE0B6)$e[48;2;${bg}m$body$e[0m$e[38;2;${bg}m$([char]0xE0B4)$e[0m"
}

function global:__wPrompt([bool]$ok) {
    $e = [char]27
    $loc = Get-Location
    $cwd = $loc.ProviderPath
    $out = if ($global:__wFirst) { '' } else { "`n" }
    $global:__wFirst = $false
    $root = $null; $prefix = ''; $branch = $null; $ahead = 0; $behind = 0; $dirty = 0
    if ($global:__wHasGit -and $loc.Provider.Name -eq 'FileSystem') {
        $top = git rev-parse --show-toplevel 2>$null
        if ($LASTEXITCODE -eq 0 -and $top) {
            $root = $top
            $prefix = git rev-parse --show-prefix 2>$null
            foreach ($l in (git status --porcelain=v2 --branch 2>$null)) {
                if ($l.StartsWith('# branch.head ')) { $branch = $l.Substring(14) }
                elseif ($l.StartsWith('# branch.ab ')) {
                    $p = $l.Substring(12).Split(' ')
                    $ahead = [int]$p[0].TrimStart('+'); $behind = [int]$p[1].TrimStart('-')
                }
                elseif (-not $l.StartsWith('#')) { $dirty++ }
            }
        }
    }
    if ($root) {
        # Dentro de un repo: nombre del proyecto + subcarpeta, rama y estado.
        $name = Split-Path $root -Leaf
        $rel = ''
        if ($prefix) { $rel = '\' + ($prefix.TrimEnd('/') -replace '/', '\') }
        $out += __wPill '{{RED_BG}}' "$e[38;2;{{RED}}m$([char]0xF401) $e[1m$name$e[22m$e[38;2;{{MUTED}}m$rel"
        if ($branch) {
            $out += ' ' + (__wPill '{{VIOLET_BG}}' "$e[38;2;{{VIOLET}}m$([char]0xE0A0) $branch")
        }
        if ($ahead -gt 0 -or $behind -gt 0) {
            $ab = @()
            if ($ahead -gt 0) { $ab += "$([char]0x2191)$ahead" }
            if ($behind -gt 0) { $ab += "$([char]0x2193)$behind" }
            $out += ' ' + (__wPill '{{YELLOW_BG}}' "$e[38;2;{{YELLOW}}m$($ab -join ' ')")
        }
        if ($dirty -gt 0) { $out += ' ' + (__wPill '{{CYAN_BG}}' "$e[38;2;{{CYAN}}m~$dirty") }
    } else {
        # Fuera de un repo: carpeta abreviada (las dos últimas).
        $p = $cwd; $icon = [char]0xF07C
        if ($HOME -and $p.StartsWith($HOME)) {
            $p = '~' + $p.Substring($HOME.Length)
            if ($p -eq '~') { $icon = [char]0xF015 }
        }
        $parts = @($p.Split('\') | Where-Object { $_ })
        if ($parts.Count -gt 3) { $p = [string][char]0x2026 + '\' + ($parts[-2..-1] -join '\') }
        $out += __wPill '{{RED_BG}}' "$e[38;2;{{RED}}m$icon $p"
    }
    $arrow = if ($ok) { '{{RED}}' } else { '{{ORANGE}}' }
    $out += "`n$e[38;2;${arrow}m$([char]0x276F)$e[0m "
    return $out
}

function global:prompt {
    $ok = $?
    $code = $global:LASTEXITCODE
    if ($global:__wOwnPrompt) {
        $p = __wPrompt $ok
    } else {
        $p = & $global:__wOrigPrompt
    }
    try {
        $cwd = (Get-Location).ProviderPath
        # Cuántas líneas ocupa el prompt (para "copiar salida del último comando").
        $lines = @("$p" -split "`n").Count
        $last = Get-History -Count 1
        # El primer elemento del historial es este mismo script de Neon
        # (va como -EncodedCommand): no se reporta como comando.
        if ($last -and $last.Id -ne $global:__wLastId -and $last.CommandLine -like '*__wLog*') {
            $global:__wLastId = $last.Id
        }
        if ($last -and $last.Id -ne $global:__wLastId) {
            $global:__wLastId = $last.Id
            $evt = [ordered]@{
                kind         = 'command'
                command      = $last.CommandLine
                cwd          = $global:__wPrevCwd
                new_cwd      = $cwd
                ok           = $ok
                exit_code    = $code
                duration_ms  = [int64]($last.EndExecutionTime - $last.StartExecutionTime).TotalMilliseconds
                ts           = [DateTimeOffset]::Now.ToUnixTimeSeconds()
                prompt_lines = $lines
            }
        } else {
            $evt = [ordered]@{ kind = 'prompt'; cwd = $cwd; ts = [DateTimeOffset]::Now.ToUnixTimeSeconds(); prompt_lines = $lines }
        }
        $global:__wPrevCwd = $cwd
        if ($global:__wLog) {
            [IO.File]::AppendAllText($global:__wLog, ($evt | ConvertTo-Json -Compress) + "`n")
        }
    } catch {}
    $global:LASTEXITCODE = $code
    return $p
}
"#;

/// Para bash (Linux), Git Bash y WSL.
const BASH_HOOK: &str = r##"
# Generado por Neon: prompt y reporte de cada comando a la terminal.
if [ -n "$NEON_GITBASH" ] && [ -f /etc/profile ]; then source /etc/profile; fi
[ -f ~/.bashrc ] && source ~/.bashrc
__w_esc() { local s=${1//\\/\\\\}; s=${s//\"/\\\"}; s=${s//$'\t'/\\t}; s=${s//$'\n'/\\n}; printf '%s' "$s"; }
# Carpeta actual en formato Windows cuando corresponde (Git Bash / WSL).
__w_pwd() {
    if [ -n "$NEON_GITBASH" ] && command -v cygpath >/dev/null 2>&1; then cygpath -w "$PWD"
    elif [ -n "$NEON_WSL" ] && command -v wslpath >/dev/null 2>&1; then wslpath -w "$PWD" 2>/dev/null || printf '%s' "$PWD"
    else printf '%s' "$PWD"; fi
}
__w_prev_cwd="$(__w_pwd)"
__w_started=""
__w_last_hist=""
__w_prompt_lines() {
    local t=${PS1//\\n/$'\x01'}; t=${t//$'\n'/$'\x01'}; t=${t//[^$'\x01']/}
    printf '%d' $(( ${#t} + 1 ))
}
__w_hook() {
    local code=$? ts; ts=$(date +%s)
    local line; line=$(HISTTIMEFORMAT= history 1)
    local cmd; cmd=$(printf '%s' "$line" | sed -E 's/^ *[0-9]+\*? *//')
    local cwd; cwd=$(__w_pwd)
    local pl; pl=$(__w_prompt_lines)
    if [ -n "$__w_started" ] && [ "$line" != "$__w_last_hist" ] && [ -n "$cmd" ]; then
        local ok=true; [ $code -ne 0 ] && ok=false
        printf '{"kind":"command","command":"%s","cwd":"%s","new_cwd":"%s","ok":%s,"exit_code":%d,"ts":%d,"prompt_lines":%d}\n' \
            "$(__w_esc "$cmd")" "$(__w_esc "$__w_prev_cwd")" "$(__w_esc "$cwd")" $ok $code $ts $pl >> "$NEON_EVENTS"
    else
        printf '{"kind":"prompt","cwd":"%s","ts":%d,"prompt_lines":%d}\n' "$(__w_esc "$cwd")" $ts $pl >> "$NEON_EVENTS"
    fi
    __w_started=1
    __w_last_hist="$line"
    __w_prev_cwd="$cwd"
    return $code
}
PROMPT_COMMAND="__w_hook${PROMPT_COMMAND:+; $PROMPT_COMMAND}"

# Prompt con cápsulas redondeadas (igual que en PowerShell). Glifos de la Nerd Font.
__w_L=$'\xee\x82\xb6'; __w_R=$'\xee\x82\xb4'; __w_BR=$'\xee\x82\xa0'; __w_REPO=$'\xef\x90\x81'
__w_DIR=$'\xef\x81\xbc'; __w_HOMEI=$'\xef\x80\x95'; __w_ARROW=$'\xe2\x9d\xaf'
__w_UP=$'\xe2\x86\x91'; __w_DN=$'\xe2\x86\x93'; __w_ELL=$'\xe2\x80\xa6'
__w_pill() { printf '\\[\\e[38;2;%sm\\]%s\\[\\e[48;2;%sm\\]%s\\[\\e[0m\\e[38;2;%sm\\]%s\\[\\e[0m\\]' "$1" "$__w_L" "$1" "$2" "$1" "$__w_R"; }
__w_ps1() {
    local code=$? first=${__w_ps1_done:-}
    __w_ps1_done=1
    local top prefix branch="" ahead=0 behind=0 dirty=0 out="" l
    top=$(git rev-parse --show-toplevel 2>/dev/null)
    if [ -n "$top" ]; then
        prefix=$(git rev-parse --show-prefix 2>/dev/null)
        while IFS= read -r l; do
            case "$l" in
                "# branch.head "*) branch=${l#"# branch.head "} ;;
                "# branch.ab "*) l=${l#"# branch.ab "}; ahead=${l%% *}; ahead=${ahead#+}; behind=${l##* }; behind=${behind#-} ;;
                "#"*) ;;
                *) dirty=$((dirty+1)) ;;
            esac
        done < <(git status --porcelain=v2 --branch 2>/dev/null)
        local name=${top##*/} rel=""
        [ -n "$prefix" ] && rel="/${prefix%/}"
        out+=$(__w_pill '{{RED_BG}}' "\\[\\e[38;2;{{RED}}m\\]$__w_REPO \\[\\e[1m\\]$name\\[\\e[22m\\e[38;2;{{MUTED}}m\\]$rel")
        [ -n "$branch" ] && out+=" $(__w_pill '{{VIOLET_BG}}' "\\[\\e[38;2;{{VIOLET}}m\\]$__w_BR $branch")"
        if [ "${ahead:-0}" -gt 0 ] || [ "${behind:-0}" -gt 0 ]; then
            local ab=""
            [ "$ahead" -gt 0 ] && ab="$__w_UP$ahead"
            [ "$behind" -gt 0 ] && ab="${ab:+$ab }$__w_DN$behind"
            out+=" $(__w_pill '{{YELLOW_BG}}' "\\[\\e[38;2;{{YELLOW}}m\\]$ab")"
        fi
        [ "$dirty" -gt 0 ] && out+=" $(__w_pill '{{CYAN_BG}}' "\\[\\e[38;2;{{CYAN}}m\\]~$dirty")"
    else
        local p=$PWD icon=$__w_DIR
        case "$p" in
            "$HOME") p="~"; icon=$__w_HOMEI ;;
            "$HOME"/*) p="~${p#"$HOME"}" ;;
        esac
        local IFS=/; local -a parts=(${p#/}); unset IFS
        [ ${#parts[@]} -gt 3 ] && p="$__w_ELL/${parts[-2]}/${parts[-1]}"
        out+=$(__w_pill '{{RED_BG}}' "\\[\\e[38;2;{{RED}}m\\]$icon $p")
    fi
    local arrow="{{RED}}"; [ $code -ne 0 ] && arrow="{{ORANGE}}"
    PS1="${first:+\n}$out\n\[\e[38;2;${arrow}m\]$__w_ARROW\[\e[0m\] "
    return $code
}
[ "$NEON_PROMPT" != "0" ] && PROMPT_COMMAND="__w_ps1; $PROMPT_COMMAND"
"##;

/// Prompt para el Símbolo del sistema (cmd no permite reportar comandos,
/// pero sí un prompt con colores).
fn cmd_prompt() -> String {
    let t = render_hook(
        "$E[38;2;{{RED_BG}}m\u{E0B6}$E[48;2;{{RED_BG}}m$E[38;2;{{RED}}m\u{F07C} $P$E[0m$E[38;2;{{RED_BG}}m\u{E0B4}$E[0m$_$E[38;2;{{RED}}m\u{276F}$E[0m ",
    );
    format!("$_{t}")
}

/// Reemplaza {{COLOR}} / {{COLOR_BG}} por "r;g;b" del tema.
fn render_hook(template: &str) -> String {
    use crate::theme::{mix, pal};
    let p = pal();
    let rgb = |c: egui::Color32| format!("{};{};{}", c.r(), c.g(), c.b());
    let mut out = template.to_string();
    for (name, c) in [
        ("RED", p.red),
        ("ORANGE", p.orange),
        ("YELLOW", p.yellow),
        ("GREEN", p.green),
        ("CYAN", p.cyan),
        ("BLUE", p.blue),
        ("VIOLET", p.violet),
        ("TEXT", p.text),
        ("MUTED", p.muted),
    ] {
        // El fondo de la cápsula es el color mezclado con el fondo del panel.
        out = out.replace(&format!("{{{{{name}_BG}}}}"), &rgb(mix(p.panel, c, 0.18)));
        out = out.replace(&format!("{{{{{name}}}}}"), &rgb(c));
    }
    out
}

/// Arma la configuración para abrir `profile` en `cwd`, con su integración.
pub fn backend_settings(
    profile: &crate::profiles::Profile,
    events_path: &Path,
    cwd: Option<PathBuf>,
) -> BackendSettings {
    use crate::profiles::{ProfileKind, to_forward_slashes, to_wsl_path};
    let dir = data_dir();
    let _ = std::fs::create_dir_all(&dir);

    let mut env = HashMap::new();
    env.insert(
        "NEON_EVENTS".to_string(),
        events_path.to_string_lossy().into_owned(),
    );
    env.insert("TERM_PROGRAM".to_string(), "neon".to_string());
    env.insert("COLORTERM".to_string(), "truecolor".to_string());

    let bash_hook = || {
        let hook = dir.join("hook.bash");
        let _ = std::fs::write(&hook, render_hook(BASH_HOOK));
        hook
    };
    let exe = profile.exe.to_string_lossy().into_owned();
    let mut working_directory = cwd.clone();

    let args = match profile.kind {
        ProfileKind::PowerShell => {
            // Se pasa como -EncodedCommand (UTF-16LE en base64) en vez de un .ps1:
            // así no lo bloquea la política de ejecución de Windows PowerShell.
            let hook = render_hook(PWSH_HOOK);
            let _ = std::fs::write(dir.join("hook.ps1"), &hook); // copia de referencia
            vec![
                "-NoLogo".into(),
                "-NoExit".into(),
                "-EncodedCommand".into(),
                encode_pwsh_command(&hook),
            ]
        }
        ProfileKind::Cmd => {
            env.insert("PROMPT".into(), cmd_prompt());
            vec![]
        }
        ProfileKind::GitBash => {
            env.insert("NEON_GITBASH".into(), "1".into());
            env.insert("MSYSTEM".into(), "MINGW64".into());
            // Que /etc/profile no salte a la carpeta personal.
            env.insert("CHERE_INVOKING".into(), "1".into());
            env.insert("NEON_EVENTS".into(), to_forward_slashes(events_path));
            vec![
                "--rcfile".into(),
                to_forward_slashes(&bash_hook()),
                "-i".into(),
            ]
        }
        ProfileKind::Wsl => {
            env.insert("NEON_WSL".into(), "1".into());
            // WSLENV pasa variables a Linux; "/p" traduce la ruta a /mnt/c/...
            env.insert(
                "WSLENV".into(),
                "NEON_EVENTS/p:NEON_WSL:NEON_PROMPT:TERM_PROGRAM:COLORTERM".into(),
            );
            let mut a: Vec<String> = vec![];
            if let Some(c) = &cwd {
                a.push("--cd".into());
                a.push(c.to_string_lossy().into_owned());
            }
            working_directory = None;
            a.extend([
                "--".into(),
                "bash".into(),
                "--rcfile".into(),
                to_wsl_path(&bash_hook()),
                "-i".into(),
            ]);
            a
        }
        ProfileKind::Bash => vec![
            "--rcfile".into(),
            bash_hook().to_string_lossy().into_owned(),
            "-i".into(),
        ],
    };

    let settings = crate::settings::get();
    if !settings.neon_prompt {
        env.insert("NEON_PROMPT".into(), "0".into());
    }
    BackendSettings {
        shell: exe,
        args,
        working_directory,
        env,
        scrollback: settings.scrollback.clamp(500, 200_000),
    }
}

/// Formato de `-EncodedCommand`: UTF-16LE en base64.
pub(crate) fn encode_pwsh_command(script: &str) -> String {
    base64(
        &script
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<u8>>(),
    )
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ShellEvent {
    pub kind: String,
    pub command: Option<String>,
    pub cwd: Option<String>,
    pub new_cwd: Option<String>,
    pub ok: Option<bool>,
    pub exit_code: Option<i64>,
    pub duration_ms: Option<i64>,
    pub ts: Option<i64>,
    /// Líneas que ocupa el prompt (para ubicar la salida del último comando).
    pub prompt_lines: Option<u32>,
}

/// Lee el archivo de eventos de una sesión de forma incremental.
pub struct SessionLog {
    pub path: PathBuf,
    offset: u64,
    partial: String,
}

impl SessionLog {
    pub fn new(path: PathBuf) -> Self {
        if let Some(d) = path.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = File::create(&path);
        Self {
            path,
            offset: 0,
            partial: String::new(),
        }
    }

    pub fn poll(&mut self) -> Vec<ShellEvent> {
        let Ok(mut f) = File::open(&self.path) else {
            return vec![];
        };
        let len = f.metadata().map(|m| m.len()).unwrap_or(0);
        if len <= self.offset {
            return vec![];
        }
        let mut buf = Vec::new();
        if f.seek(SeekFrom::Start(self.offset)).is_err() || f.read_to_end(&mut buf).is_err() {
            return vec![];
        }
        self.offset += buf.len() as u64;
        self.partial.push_str(&String::from_utf8_lossy(&buf));

        let mut events = vec![];
        while let Some(nl) = self.partial.find('\n') {
            let line: String = self.partial.drain(..=nl).collect();
            let line = line.trim().trim_start_matches('\u{feff}');
            if line.is_empty() {
                continue;
            }
            if let Ok(e) = serde_json::from_str::<ShellEvent>(line) {
                events.push(e);
            }
        }
        events
    }
}

impl Drop for SessionLog {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn hooks_have_no_leftover_placeholders() {
        for h in [render_hook(PWSH_HOOK), render_hook(BASH_HOOK)] {
            assert!(!h.contains("{{"), "quedó un marcador sin reemplazar");
        }
        assert!(render_hook("{{RED}}|{{RED_BG}}").starts_with("237;74;91|"));
        assert!(!cmd_prompt().contains("{{"));
    }

    /// Lo que realmente recibe PowerShell no debe tener marcadores sin reemplazar
    /// (este error llegó a Windows: el prompt mostraba "{RED_BG}").
    #[test]
    fn encoded_powershell_command_is_rendered() {
        let hook = render_hook(PWSH_HOOK);
        let enc = encode_pwsh_command(&hook);
        let expected = base64(
            &hook
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<u8>>(),
        );
        assert_eq!(enc, expected);
        assert!(!hook.contains("{{") && hook.contains("38;2;237;74;91m"));
        assert!(!encode_pwsh_command(PWSH_HOOK).is_empty());
    }

    #[test]
    fn base64_matches_reference() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"hola mundo!"), "aG9sYSBtdW5kbyE=");
    }

    #[test]
    fn reads_incrementally_and_handles_partial_lines() {
        let path = std::env::temp_dir().join(format!("neon-test-{}.jsonl", std::process::id()));
        let mut log = SessionLog::new(path.clone());
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        write!(f, "\u{feff}{{\"kind\":\"command\",\"command\":\"git status\",\"cwd\":\"C:\\\\x\",\"ok\":true,\"exit_code\":null,\"duration_ms\":12,\"ts\":5}}\n{{\"kind\":\"pro").unwrap();
        let ev = log.poll();
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].command.as_deref(), Some("git status"));
        assert_eq!(ev[0].cwd.as_deref(), Some("C:\\x"));
        write!(f, "mpt\",\"cwd\":\"/tmp\",\"ts\":6}}\n").unwrap();
        let ev = log.poll();
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].kind, "prompt");
        assert!(log.poll().is_empty());
        drop(log);
        assert!(!path.exists());
    }
}
