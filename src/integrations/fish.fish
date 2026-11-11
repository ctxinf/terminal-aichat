# terminal-aichat fish integration
#
# Provides:
#   q  <message>    ask AI; if reply is a command, put it into the buffer
#   qe <message>    ask AI; if a command is parsed, echo it as `# …`
#                   comments then execute it; if parsing fails, dump the
#                   reply as chat text (no execution)
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
        jq -ers 'if length == 1 and (.[0] | type == "object") then .[0].command | select(type == "string") | select(test("[^[:space:]]") and (explode | index(0) == null)) else empty end' 2>/dev/null
    else if command -q python3
        python3 -c 'import sys,json
try:
  d=json.load(sys.stdin)
  v=d.get("command") if isinstance(d,dict) else None
  if not isinstance(v,str) or not v.strip() or chr(0) in v: sys.exit(1)
  sys.stdout.write(v)
except Exception: sys.exit(1)' 2>/dev/null
    else
        printf '%s\n' 'aichat: install jq or python3 to enable command replies.' >&2
        return 1
    end
end

function _aichat_run
    env -u TERMINAL_AICHT_INIT_INTEGRATION \
        TERMINAL_AICHT_PROMPT={{PROMPT_NAME}} \
        TERMINAL_AICHT_CONFIG={{CONFIG_PATH}} \
        TERMINAL_AICHT_PURE=true TERMINAL_AICHT_DISABLE_STREAM=true \
        aichat --integration-shell {{SHELL_NAME}} -- $argv
end

function _aichat_capture
    _aichat_run $argv | string collect
    return $pipestatus[1]
end

function _aichat_show_chat
    if isatty stdout
        set_color cyan
        printf 'aichat\n'
        set_color normal
    end
    printf '%s\n' "$argv[1]"
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

    set -l output (_aichat_capture $args)
    or return $status
    set -l cmd (printf '%s' "$output" | _aichat_extract_cmd | string collect)

    if test -n "$cmd"
        if not isatty stdout
            printf '%s\n' "$cmd"
            return 0
        end
        if test $debug -eq 1
            set -l comment (printf '%s' "$output" | sed 's/^/# /' | tr '\n' ' ')
            commandline -r "$comment
$cmd"
        else
            commandline -r -- "$cmd"
        end
    else
        _aichat_show_chat "$output"
    end
end

function _aichat_ask_exec
    if test (count $argv) -eq 0; and isatty stdin
        echo "usage: qe <message>   (or:  cmd | qe [message])" >&2
        return 1
    end

    set -l output (_aichat_capture $argv)
    or return $status
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
        # Plain text or an invalid command reply is displayed without execution.
        _aichat_show_chat "$output"
    end
end

# Abbreviations expand on space/enter, so `? foo` becomes `q foo`.
# `--position command` keeps the abbreviation from firing mid-line.
abbr -a -g --position command -- '?' q 2>/dev/null
abbr -a -g --position command -- '?!' qe 2>/dev/null
