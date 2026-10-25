Pod::Spec.new do |s|
  s.name = 'CanopyRemote'
  s.version = '1.0.0'
  s.summary = 'Canopy native gRPC transport'
  s.description = 'Native HTTP/2 gRPC calls with pinned host trust and client certificates.'
  s.license = { :type => 'MIT' }
  s.author = 'Canopy'
  s.homepage = 'https://github.com/itsol-tech/canopy'
  s.platforms = { :ios => '16.0' }
  s.source = { :git => '' }
  s.static_framework = true
  s.dependency 'ExpoModulesCore'
  s.dependency 'libssh2-iosx', '1.11.1.0'
  s.dependency 'openssl-iosx', '3.5.9.2'
  s.dependency 'gRPC-Core', '1.71.0'
  s.dependency 'BoringSSL-GRPC', '0.0.39'
  s.compiler_flags = '-DBORINGSSL_PREFIX=GRPC'
  s.source_files = '**/*.{h,m,mm,swift}'
  s.resource_bundles = { 'CanopyRemote' => ['Methods.json'] }
  s.pod_target_xcconfig = { 'DEFINES_MODULE' => 'YES', 'CLANG_CXX_LANGUAGE_STANDARD' => 'c++17' }
end
