/**
 * Environment variables that must never be overridden by user-supplied customEnv.
 * All entries UPPERCASE — callers must normalize keys with .toUpperCase() before checking.
 * Covers: system paths, dynamic linkers, shell startup, language runtimes, proxies, SSH/Git,
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

  // Node / Electron
  'NODE_OPTIONS',
  'NODE_EXTRA_CA_CERTS',
  'ELECTRON_RUN_AS_NODE',

  // Shell startup files (sourced when a non-interactive bash / any zsh starts)
  'BASH_ENV',
  'ZDOTDIR',

  // Language runtimes
  'PYTHONPATH',
  'PYTHONHOME',
  'RUBYLIB',
  'RUBYOPT',
  'PERL5LIB',
  'PERL5OPT',
  'CLASSPATH',
  'JAVA_TOOL_OPTIONS',
  '_JAVA_OPTIONS',

  // Git / SSH
  'GIT_SSH_COMMAND',
  'GIT_ASKPASS',
  'SSH_AUTH_SOCK',
  'GIT_EXEC_PATH',
  'GIT_EXTERNAL_DIFF',
  // Env-injected git config (`GIT_CONFIG_COUNT` + `GIT_CONFIG_KEY_n`/`VALUE_n`) would set
  // core.sshCommand and friends, bypassing the GIT_SSH_COMMAND block above.
  'GIT_CONFIG_COUNT',
  'GIT_CONFIG_PARAMETERS',

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
