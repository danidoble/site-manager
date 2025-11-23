import Store from 'electron-store'

export interface Site {
  domain: string
  type: 'php' | 'proxy'
  phpVersion?: string
  proxyPort?: number
}

const store = new Store<{ sites: Site[] }>({
  defaults: {
    sites: [],
  },
})

export default store
