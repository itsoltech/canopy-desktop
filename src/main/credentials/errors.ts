import { match } from 'ts-pattern'

export type CredentialError =
  | { _tag: 'CredentialUnknown'; credentialId: string }
  | { _tag: 'CredentialProviderUnsupported'; provider: string }
  | { _tag: 'CredentialCapabilityUnsupported'; provider: string; capability: string }
  | { _tag: 'CredentialNotFound' }
  | { _tag: 'CredentialAmbiguous'; candidateCount: number }
  | { _tag: 'CredentialBindingIncompatible'; bindingKey: string }
  | { _tag: 'CredentialApprovalRequired'; bindingKey: string }
  | { _tag: 'CredentialSecretMissing'; credentialId: string }

export function credentialErrorMessage(error: CredentialError): string {
  return match(error)
    .with(
      { _tag: 'CredentialUnknown' },
      (e) => `Stored credential ${e.credentialId} no longer exists`,
    )
    .with(
      { _tag: 'CredentialProviderUnsupported' },
      (e) => `Unsupported credential provider: ${e.provider}`,
    )
    .with(
      { _tag: 'CredentialCapabilityUnsupported' },
      (e) => `${e.provider} credentials do not support ${e.capability}`,
    )
    .with({ _tag: 'CredentialNotFound' }, () => 'No compatible credential is stored')
    .with(
      { _tag: 'CredentialAmbiguous' },
      () => 'Multiple compatible credentials are stored; re-enter the intended token to bind it',
    )
    .with(
      { _tag: 'CredentialBindingIncompatible' },
      () => 'The credential bound to this integration is not compatible',
    )
    .with(
      { _tag: 'CredentialApprovalRequired' },
      () => 'Explicit approval is required before this integration can use the credential',
    )
    .with(
      { _tag: 'CredentialSecretMissing' },
      () => 'The stored credential secret is missing; re-enter the token',
    )
    .exhaustive()
}
