/**
 * Environment variables that must never be overridden by user-supplied customEnv.
 * Covers: system paths, dynamic linkers, language runtimes, proxies, SSH/Git, editors, Node/Electron.
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

  // Language runtimes
  'PYTHONPATH',
  'PYTHONHOME',
  'RUBYLIB',
  'PERL5LIB',
  'CLASSPATH',

  // Git / SSH
  'GIT_SSH_COMMAND',
  'GIT_ASKPASS',
  'SSH_AUTH_SOCK',

  // Editors (can execute arbitrary commands)
  'EDITOR',
  'VISUAL',

  // Proxies / TLS
  'http_proxy',
  'https_proxy',
  'HTTP_PROXY',
  'HTTPS_PROXY',
  'no_proxy',
  'NO_PROXY',
  'SSL_CERT_FILE',
  'SSL_CERT_DIR',

  // Build / compilation
  'CC',
  'CXX',
  'LDFLAGS',
  'CFLAGS',
])
