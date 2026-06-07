# terminal-aichat fish integration
#
# Provides:
#   q  <message>    ask AI; if reply is a command, put it into the buffer
#   qe <message>    ask AI; if a command is parsed, echo it as `# …`
#                   comments then execute it; if parsing fails, dump the
#                   full reply between `###` markers (no execution)
#   command | q     pipe stdin as additional context
#   q --debug ...   prepend the raw model reply as a comment line (`q` only)
#
# Fish does not allow `?` in function names. The script also defines
# abbreviations `?` and `?!` that expand to `q` / `qe`.
#
# Install:
#   aichat --init-integration fish | source
#
# To use a custom prompt name (will be added to your config under `prompts`):
#   aichat --init-integration fish --prompt <name> | source

function _aichat_extract_cmd
    if command -q jq
        jq -r '.command? // empty' 2>/dev/null
    else if command -q python3
        python3 -c 'import sys,json
try:
  d=json.load(sys.stdin); v=d.get("command")
  print(v if isinstance(v,str) else "")
except Exception: pass' 2>/dev/null
    else
        sed -n 's/.*"command"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n1
    end
end

function _aichat_run
    command aichat --prompt {{PROMPT_NAME}} --pure --disable-stream $argv
end

function _aichat_ask
    set -l debug 0
    set -l args
    for a in $argv
        if test "$a" = "--debug"
            set debug 1
        else
            set -a args $a
        end
    end
    if test (count $args) -eq 0; and isatty stdin
        echo "usage: q <message>   (or:  cmd | q [message])" >&2
        return 1
    end

    set -l output (_aichat_run $args | string collect)
    set -l cmd (printf '%s' "$output" | _aichat_extract_cmd | string collect)

    if test -n "$cmd"
        if test $debug -eq 1
            set -l comment (printf '%s' "$output" | sed 's/^/# /' | tr '\n' ' ')
            commandline -r "$comment
$cmd"
        else
            commandline -r -- "$cmd"
        end
    else
        echo "###############################################"
        printf '%s\n' "$output"
        echo "###############################################"
    end
end

function _aichat_ask_exec
    if test (count $argv) -eq 0; and isatty stdin
        echo "usage: qe <message>   (or:  cmd | qe [message])" >&2
        return 1
    end

    set -l output (_aichat_run $argv | string collect)
    set -l cmd (printf '%s' "$output" | _aichat_extract_cmd | string collect)

    if test -n "$cmd"
        # Parse succeeded: echo the resolved command as `# …` comments before
        # executing (multi-line aware). Running model-generated commands is risky;
        # this gives the user a last chance to spot something wrong.
        # When stdout is a tty, color the comment lines like a real shell comment.
        if isatty stdout
            set_color brblack
            printf '%s\n' "$cmd" | sed 's/^/# /'
            set_color normal
        else
            printf '%s\n' "$cmd" | sed 's/^/# /'
        end
        eval $cmd
    else
        # Parse failed: dump the full model reply between markers.
        echo "###############################################"
        printf '%s\n' "$output"
        echo "###############################################"
    end
end

# Abbreviations expand on space/enter, so `? foo` becomes `q foo`.
# `--position command` keeps the abbreviation from firing mid-line.
abbr -a -g --position command -- '?' q 2>/dev/null
abbr -a -g --position command -- '?!' qe 2>/dev/null
