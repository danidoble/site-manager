export {}

declare global {
  interface Window {
    ipcRenderer: {
      invoke(channel: 'check-dependencies'): Promise<any>
      invoke(channel: 'install-dependencies'): Promise<any>
      invoke(channel: 'get-php-versions'): Promise<string[]>
      invoke(channel: 'get-sites'): Promise<any[]>
      invoke(channel: 'create-site', config: any): Promise<void>
      invoke(channel: 'update-site', domain: string, config: any): Promise<void>
      invoke(channel: 'delete-site', domain: string): Promise<void>
      invoke(channel: 'regenerate-ca'): Promise<void>
      invoke(channel: 'regenerate-site-cert', domain: string): Promise<void>
    }
  }
}
