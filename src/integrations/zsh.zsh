# terminal-aichat zsh integration
#
# Provides:
#   ? <message>     ask AI; if reply is a command, put it into the buffer
#   ?! <message>    ask AI; if a command is parsed, echo it as `# …`
#                   comments then execute it; if parsing fails, dump the
#                   full reply between `###` markers (no execution)
#   command | ?     pipe stdin as additional context
#   ? --debug ...   prepend the raw model reply as a comment line (`?` only)
#
# Install:
#   eval "$(aichat --init-integration zsh)"
#
# To use a custom prompt name (will be added to your config under `prompts`):
#   eval "$(aichat --init-integration zsh --prompt <name>)"

# Extract .command from JSON reply. Tries jq, then python3, then a sed fallback.
_aichat_extract_cmd() {
  if command -v jq >/dev/null 2>&1; then
    jq -r '.command? // empty' 2>/dev/null
  elif command -v python3 >/dev/null 2>&1; then
    python3 -c $'import sys,json\ntry:\n  d=json.load(sys.stdin); v=d.get("command");\n  print(v if isinstance(v,str) else "")\nexcept Exception: pass' 2>/dev/null
  else
    sed -n 's/.*"command"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n1
  fi
}

# Run aichat with the shell-exec-or-chat prompt and collect output.
# The prompt itself lives in your config file (under prompts.{{PROMPT_NAME}})
# so you can read and edit it — run `aichat -l` to see the location.
_aichat_run() {
  command aichat --prompt {{PROMPT_NAME}} --pure --disable-stream "$@"
}

_aichat_ask() {
  emulate -L zsh
  local debug=0
  local -a args
  args=()
  local arg
  for arg in "$@"; do
    if [[ "$arg" == "--debug" ]]; then
      debug=1
    else
      args+=("$arg")
    fi
  done
  if (( ${#args[@]} == 0 )) && [[ -t 0 ]]; then
    print -u2 "usage: ? <message>   (or:  cmd | ? [message])"
    return 1
  fi

  local output cmd
  output=$(_aichat_run "${args[@]}")
  cmd=$(printf '%s' "$output" | _aichat_extract_cmd)

  if [[ -n "$cmd" ]]; then
    if (( debug )); then
      local comment
      comment=$(printf '%s' "$output" | sed 's/^/# /' | tr '\n' ' ')
      print -z "${comment}
$cmd"
    else
      print -z -- "$cmd"
    fi
  else
    print -- "###############################################"
    print -r -- "$output"
    print -- "###############################################"
  fi
}

_aichat_ask_exec() {
  emulate -L zsh
  if (( $# == 0 )) && [[ -t 0 ]]; then
    print -u2 "usage: ?! <message>   (or:  cmd | ?! [message])"
    return 1
  fi

  local output cmd
  output=$(_aichat_run "$@")
  cmd=$(printf '%s' "$output" | _aichat_extract_cmd)

  if [[ -n "$cmd" ]]; then
    # Parse succeeded: echo the resolved command as `# …` comments before
    # executing (multi-line aware). Running model-generated commands is risky;
    # this gives the user a last chance to spot something wrong.
    # When stdout is a tty, color the comment lines like a real shell comment
    # (bright-black / dim grey, matching most syntax-highlighting themes).
    if [[ -t 1 ]]; then
      local _dim=$'\e[90m' _rst=$'\e[0m'
      printf '%s\n' "$cmd" | sed "s/^/${_dim}# /;s/\$/${_rst}/"
    else
      printf '%s\n' "$cmd" | sed 's/^/# /'
    fi
    print -s -- "$cmd"
    eval "$cmd"
  else
    # Parse failed: dump the full model reply between markers.
    print -- "###############################################"
    print -r -- "$output"
    print -- "###############################################"
  fi
}

# Use `noglob` so `?` and `*` in user input aren't expanded by zsh globbing.
alias '?'='noglob _aichat_ask'
alias '?!'='noglob _aichat_ask_exec'
