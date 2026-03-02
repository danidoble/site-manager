export {}

declare global {
  interface SiteConfig {
    domain: string
    type: 'php' | 'proxy'
    phpVersion?: string
    proxyPort?: number
    aliases?: string[]
  }

  interface DependenciesConfig {
    nginx: boolean
    php: boolean
    openssl: boolean
    certutil: boolean
  }

  interface Window {
    ipcRenderer: {
      invoke(channel: 'check-dependencies'): Promise<DependenciesConfig>
      invoke(channel: 'install-dependencies'): Promise<unknown>
      invoke(channel: 'get-php-versions'): Promise<string[]>
      invoke(channel: 'get-sites'): Promise<SiteConfig[]>
      invoke(channel: 'create-site', config: Omit<SiteConfig, 'domain'> & { domain: string }): Promise<void>
      invoke(channel: 'update-site', domain: string, config: Partial<SiteConfig>): Promise<void>
      invoke(channel: 'delete-site', domain: string): Promise<void>
      invoke(channel: 'regenerate-ca'): Promise<void>
      invoke(channel: 'regenerate-site-cert', domain: string): Promise<void>
      invoke(channel: 'get-hosts'): Promise<string>
      invoke(channel: 'format-hosts'): Promise<void>
    }
  }
}
