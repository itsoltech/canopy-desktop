/**
 * Environment variables that must never be overridden by user-supplied customEnv.
 * All entries UPPERCASE — callers must normalize keys with .toUpperCase() before checking.
 * Covers: system paths, dynamic linkers, shell startup hooks, language runtimes, proxies, SSH/Git,
 * editors, Node/Electron.
 */
export const BLOCKED_ENV_VARS = new Set([
  // System
  'PATH',
  'HOME',
  'USER',
  'SHELL',
  'TERM',

  // Dynamic linker (Linux)
  'LD_PRELOAD',
  'LD_LIBRARY_PATH',
  'LD_AUDIT',

  // Dynamic linker (macOS)
  'DYLD_INSERT_LIBRARIES',
  'DYLD_LIBRARY_PATH',
  'DYLD_FRAMEWORK_PATH',
  'DYLD_FALLBACK_LIBRARY_PATH',
  'DYLD_FALLBACK_FRAMEWORK_PATH',

  // Shell startup hooks (sourced or run by the spawned shell)
  'BASH_ENV',
  'ENV',
  'PROMPT_COMMAND',

  // Node / Electron
  'NODE_OPTIONS',
  'NODE_EXTRA_CA_CERTS',
  'ELECTRON_RUN_AS_NODE',

  // Language runtimes
  'PYTHONPATH',
  'PYTHONHOME',
  'PYTHONSTARTUP',
  'RUBYLIB',
  'RUBYOPT',
  'PERL5LIB',
  'PERL5OPT',
  'CLASSPATH',
  'JAVA_TOOL_OPTIONS',
  '_JAVA_OPTIONS',

  // Git / SSH
  'GIT_SSH_COMMAND',
  'GIT_SSH',
  'GIT_ASKPASS',
  'GIT_PROXY_COMMAND',
  'GIT_EXEC_PATH',
  // Env-injected git config (e.g. core.sshCommand, core.fsmonitor). Git reads
  // the GIT_CONFIG_KEY_<n>/GIT_CONFIG_VALUE_<n> pairs only when COUNT is set.
  'GIT_CONFIG_COUNT',
  'GIT_CONFIG_GLOBAL',
  'GIT_CONFIG_SYSTEM',
  'SSH_AUTH_SOCK',

  // Editors (can execute arbitrary commands)
  'EDITOR',
  'VISUAL',

  // Proxies / TLS (callers normalize to uppercase, so lowercase variants are covered)
  'HTTP_PROXY',
  'HTTPS_PROXY',
  'ALL_PROXY',
  'FTP_PROXY',
  'NO_PROXY',
  'SSL_CERT_FILE',
  'SSL_CERT_DIR',
  // CA-bundle overrides honoured by common agent toolchains — a user/config
  // value here could route an agent's API traffic through a rogue CA/proxy.
  'REQUESTS_CA_BUNDLE',
  'CURL_CA_BUNDLE',
  'GIT_SSL_CAINFO',
  'NODE_TLS_REJECT_UNAUTHORIZED',

  // Build / compilation
  'CC',
  'CXX',
  'LDFLAGS',
  'CFLAGS',
])
