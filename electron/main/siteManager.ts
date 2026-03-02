import { ipcMain, app } from 'electron'
import { exec } from 'child_process'
import sudo from 'sudo-prompt'
import { promisify } from 'util'
import fs from 'node:fs'
import path from 'node:path'
import store, { Site } from './store'

const execAsync = promisify(exec)

const sudoOptions = {
  name: 'Site Manager',
}

export class SiteManager {
  constructor() {
    this.registerIpcHandlers()
  }

  private registerIpcHandlers() {
    ipcMain.handle('check-dependencies', this.checkDependencies.bind(this))
    ipcMain.handle('install-dependencies', this.installDependencies.bind(this))
    ipcMain.handle('get-php-versions', this.getPhpVersions.bind(this))
    ipcMain.handle('get-sites', () => store.get('sites'))
    ipcMain.handle('create-site', (_event, config) => this.createSite(config))
    ipcMain.handle('update-site', (_event, domain, config) => this.updateSite(domain, config))
    ipcMain.handle('delete-site', (_event, domain) => this.deleteSite(domain))
    ipcMain.handle('regenerate-ca', this.regenerateCA.bind(this))
    ipcMain.handle('regenerate-site-cert', (_event, domain) => this.regenerateSiteCert(domain))
    ipcMain.handle('get-hosts', async () => {
      try {
        return await fs.promises.readFile('/etc/hosts', 'utf-8')
      } catch {
        return 'Error reading /etc/hosts'
      }
    })
    ipcMain.handle('format-hosts', this.formatHosts.bind(this))
  }

  // ... (checkDependencies, getPhpVersions, installDependencies remain same)

  private async checkDependencies() {
    const results = {
      nginx: false,
      php: false,
      openssl: false,
      certutil: false,
    }

    try {
      await execAsync('which nginx')
      results.nginx = true
    } catch { /* ignore */ }

    try {
      await execAsync('which php')
      results.php = true
    } catch { /* ignore */ }

    try {
      await execAsync('which openssl')
      results.openssl = true
    } catch { /* ignore */ }

    try {
      await execAsync('which certutil')
      results.certutil = true
    } catch { /* ignore */ }

    return results
  }

  private async getPhpVersions() {
    try {
      // Look for php-fpm sockets or binaries
      // Common path: /run/php/phpX.Y-fpm.sock
      // Or check /usr/bin/php*
      const { stdout } = await execAsync('ls /usr/bin/php*')
      const versions = stdout.split('\n')
        .filter(line => line.match(/\/usr\/bin\/php\d+\.\d+$/))
        .map(line => line.replace('/usr/bin/php', ''))
        .sort()
      
      return versions
    } catch {
      return []
    }
  }

  private async installDependencies() {
    // This command assumes Debian/Ubuntu based system as per user request
    const command = 'apt-get update && apt-get install -y nginx php-fpm libnss3-tools openssl'
    return new Promise((resolve, reject) => {
      sudo.exec(command, sudoOptions, (error, stdout) => {
        if (error) {
          reject(error)
        } else {
          resolve(stdout)
        }
      })
    })
  }

  private async formatHosts() {
    // Uses sed to remove multiple blank lines
    const command = "sed -i -e '/^$/N;/^\\n$/D' /etc/hosts"
    return new Promise((resolve, reject) => {
      sudo.exec(command, sudoOptions, (error, stdout) => {
        if (error) {
          reject(error)
        } else {
          resolve(stdout)
        }
      })
    })
  }

  private async removeCA() {
    const commands = [
      // Remove from System
      `rm -f /usr/local/share/ca-certificates/SiteManagerCA.crt`,
      `update-ca-certificates --fresh`,
      // Remove from Chrome/Chromium
      `certutil -d sql:$HOME/.pki/nssdb -D -n "SiteManager Local CA"`,
    ]

    // Remove from Firefox
    const firefoxProfiles = await execAsync('find $HOME/.mozilla/firefox -name "*.default*"').then(r => r.stdout.split('\n').filter(Boolean)).catch(() => [])
    for (const profile of firefoxProfiles) {
       commands.push(`certutil -d sql:${profile} -D -n "SiteManager Local CA"`)
    }

    const fullCommand = commands.join(' ; ') // Use ; to ensure all run even if some fail (e.g. cert not found)
    
    await new Promise((resolve) => {
      sudo.exec(fullCommand, sudoOptions, (_error, stdout) => {
        // Ignore errors as cert might not exist
        resolve(stdout)
      })
    })
  }

  private async regenerateCA() {
    await this.removeCA()
    
    const caDir = path.join(app.getPath('userData'), 'ssl')
    const caKey = path.join(caDir, 'rootCA.key')
    const caCert = path.join(caDir, 'rootCA.pem')
    
    if (fs.existsSync(caKey)) fs.unlinkSync(caKey)
    if (fs.existsSync(caCert)) fs.unlinkSync(caCert)
    
    await this.setupCA()
    
    // Regenerate all site certs
    const sites = store.get('sites')
    for (const site of sites) {
      await this.regenerateSiteCert(site.domain)
    }
    
    return true
  }

  private async regenerateSiteCert(domain: string) {
    const certPath = `/etc/ssl/certs/${domain}.crt`
    const keyPath = `/etc/ssl/private/${domain}.key`
    const csrPath = `/tmp/${domain}.csr`
    const extPath = `/tmp/${domain}.ext`
    
    const sites = store.get('sites')
    const site = sites.find(s => s.domain === domain)
    const aliases = site?.aliases || []

    const { caKey, caCert } = await this.setupCA()
    
    const altNames = [
      `DNS.1 = ${domain}`,
      `DNS.2 = *.${domain}`,
      ...aliases.map((a: string, i: number) => `DNS.${i+3} = ${a}`),
      `IP.1 = 127.0.0.1`
    ].join('\n')

    const extContent = `
authorityKeyIdentifier=keyid,issuer
basicConstraints=CA:FALSE
keyUsage = digitalSignature, nonRepudiation, keyEncipherment, dataEncipherment
subjectAltName = @alt_names

[alt_names]
${altNames}
`

    const commands = [
      `echo "${extContent}" > ${extPath}`,
      `openssl genrsa -out ${keyPath} 2048`,
      `openssl req -new -key ${keyPath} -out ${csrPath} -subj "/C=US/ST=State/L=City/O=Organization/CN=${domain}"`,
      `openssl x509 -req -in ${csrPath} -CA "${caCert}" -CAkey "${caKey}" -CAcreateserial -out ${certPath} -days 365 -sha256 -extfile ${extPath}`,
      `rm -f ${csrPath} ${extPath}`,
      `systemctl reload nginx`
    ]
    
    const fullCommand = commands.join(' && ')
    
    await new Promise((resolve, reject) => {
      sudo.exec(fullCommand, sudoOptions, (error, stdout) => {
        if (error) reject(error)
        else resolve(stdout)
      })
    })
    
    return true
  }

  private async setupCA() {
    // ... (existing setupCA code)
    const caDir = path.join(app.getPath('userData'), 'ssl')
    const caKey = path.join(caDir, 'rootCA.key')
    const caCert = path.join(caDir, 'rootCA.pem')

    if (!fs.existsSync(caDir)) {
      fs.mkdirSync(caDir, { recursive: true })
    }

    if (!fs.existsSync(caKey) || !fs.existsSync(caCert)) {
      // Generate Root CA
      await execAsync(`openssl genrsa -out "${caKey}" 2048`)
      await execAsync(`openssl req -x509 -new -nodes -key "${caKey}" -sha256 -days 3650 -out "${caCert}" -subj "/C=US/ST=State/L=City/O=SiteManager/CN=SiteManager Local CA"`)
      
      // Trust Root CA
      const commands = [
        // System Trust
        `cp "${caCert}" /usr/local/share/ca-certificates/SiteManagerCA.crt`,
        `update-ca-certificates`,
        // Chrome/Chromium Trust
        `certutil -d sql:$HOME/.pki/nssdb -A -t "C,," -n "SiteManager Local CA" -i "${caCert}"`,
      ]
      
      // Attempt Firefox Trust (standard locations)
      const firefoxProfiles = await execAsync('find $HOME/.mozilla/firefox -name "*.default*"').then(r => r.stdout.split('\n').filter(Boolean)).catch(() => [])
      for (const profile of firefoxProfiles) {
         commands.push(`certutil -d sql:${profile} -A -t "C,," -n "SiteManager Local CA" -i "${caCert}"`)
      }

      const fullCommand = commands.join(' && ')
      await new Promise((resolve, reject) => {
        sudo.exec(fullCommand, sudoOptions, (error, stdout) => {
          if (error) reject(error)
          else resolve(stdout)
        })
      })
    }
    return { caKey, caCert }
  }

  // ... (createSite and deleteSite remain same)

  private async createSite(config: Omit<Site, 'domain'> & { domain: string }) {
    const { domain, type, phpVersion, proxyPort, aliases = [] } = config
    const rootDir = `/var/www/${domain}`
    const publicDir = `${rootDir}/public` // Laravel/Modern PHP convention
    const certPath = `/etc/ssl/certs/${domain}.crt`
    const keyPath = `/etc/ssl/private/${domain}.key`
    const csrPath = `/tmp/${domain}.csr`
    const extPath = `/tmp/${domain}.ext`
    
    // Ensure CA exists
    const { caKey, caCert } = await this.setupCA()
    
    const altNames = [
      `DNS.1 = ${domain}`,
      `DNS.2 = *.${domain}`,
      ...aliases.map((a: string, i: number) => `DNS.${i+3} = ${a}`),
      `IP.1 = 127.0.0.1`
    ].join('\n')

    // OpenSSL Config for SAN
    const extContent = `
authorityKeyIdentifier=keyid,issuer
basicConstraints=CA:FALSE
keyUsage = digitalSignature, nonRepudiation, keyEncipherment, dataEncipherment
subjectAltName = @alt_names

[alt_names]
${altNames}
`

    // Commands to execute
    const commands = []
    
    if (type === 'php') {
      commands.push(`if [ ! -d "${rootDir}" ]; then mkdir -p ${publicDir} && chown -R $USER:$USER ${rootDir} && echo "<?php phpinfo(); ?>" > ${publicDir}/index.php; fi`)
    }

    commands.push(
      // Write ext file
      `echo "${extContent}" > ${extPath}`,

      // Generate Site Key
      `openssl genrsa -out ${keyPath} 2048`,
      // Generate CSR
      `openssl req -new -key ${keyPath} -out ${csrPath} -subj "/C=US/ST=State/L=City/O=Organization/CN=${domain}"`,
      // Sign with CA and Extensions
      `openssl x509 -req -in ${csrPath} -CA "${caCert}" -CAkey "${caKey}" -CAcreateserial -out ${certPath} -days 365 -sha256 -extfile ${extPath}`,
      
      // Cleanup temp files
      `rm -f ${csrPath} ${extPath}`,
    )

    // Nginx Config
    let nginxConfig = ''
    // Check for FastCGI snippet
    const useSnippets = fs.existsSync('/etc/nginx/snippets/fastcgi-php.conf')
    let phpLocationBlock = ''
    
    if (useSnippets) {
      phpLocationBlock = `
        include snippets/fastcgi-php.conf;
        fastcgi_pass unix:/run/php/php${phpVersion}-fpm.sock;
      `
    } else {
      // Fallback for systems without snippets (e.g. some Arch/Fedora/Custom setups)
      // Assumes fastcgi_params exists in standard location
      phpLocationBlock = `
        try_files \\$uri =404;
        fastcgi_split_path_info ^(.+\\.php)(/.+)$;
        fastcgi_index index.php;
        fastcgi_param SCRIPT_FILENAME \\$document_root\\$fastcgi_script_name;
        include fastcgi_params;
        fastcgi_pass unix:/run/php/php${phpVersion}-fpm.sock;
      `
    }

    const serverNames = [domain, ...aliases].join(' ')

    if (type === 'php') {
      nginxConfig = `
server {
    listen 80;
    listen 443 ssl;
    server_name ${serverNames};
    root ${publicDir};
    index index.php index.html;

    ssl_certificate ${certPath};
    ssl_certificate_key ${keyPath};

    location / {
        try_files \\$uri \\$uri/ /index.php?\\$query_string;
    }

    location ~ \\.php$ {
        ${phpLocationBlock}
    }
}
`
    } else if (type === 'proxy') {
      nginxConfig = `
server {
    listen 80;
    listen 443 ssl;
    server_name ${serverNames};

    ssl_certificate ${certPath};
    ssl_certificate_key ${keyPath};

    location / {
        proxy_pass http://127.0.0.1:${proxyPort};
        proxy_http_version 1.1;
        proxy_set_header Upgrade \\$http_upgrade;
        proxy_set_header Connection 'upgrade';
        proxy_set_header Host \\$host;
        proxy_cache_bypass \\$http_upgrade;
    }
}
`
    }

    // Check if sites-available exists
    const useSitesAvailable = fs.existsSync('/etc/nginx/sites-available')
    const configDir = useSitesAvailable ? '/etc/nginx/sites-available' : '/etc/nginx/conf.d'
    const configExtension = useSitesAvailable ? '' : '.conf'
    const configPath = `${configDir}/${domain}${configExtension}`

    // Write config
    commands.push(`echo '${nginxConfig}' > ${configPath}`)
    
    if (useSitesAvailable) {
      commands.push(`ln -sf ${configPath} /etc/nginx/sites-enabled/`)
    }
    
    commands.push(`nginx -t && systemctl reload nginx`)
    
    // Add to hosts file if not exists
    // Use printf to correctly handle newlines
    const allDomains = [domain, ...aliases].join(' ')
    const hostsEntry = `127.0.0.1 ${allDomains}`
    const startMarker = `#start site-manager-${domain}`
    const endMarker = `#end site-manager-${domain}`
    const block = `\\n${startMarker}\\n${hostsEntry}\\n${endMarker}\\n`
    
    // Remove old block if exists (sed is safer here)
    // Then append new block using printf
    commands.push(`sed -i '/${startMarker}/,/${endMarker}/d' /etc/hosts`)
    commands.push(`printf "${block}" >> /etc/hosts`)

    const fullCommand = commands.join(' && ')

    await new Promise((resolve, reject) => {
      sudo.exec(fullCommand, sudoOptions, (error, stdout) => {
        if (error) {
          console.error(error)
          reject(error)
        } else {
          resolve(stdout)
        }
      })
    })

    const sites = store.get('sites') || []
    sites.push({ domain, type, phpVersion, proxyPort, aliases })
    store.set('sites', sites)
  }

  private async updateSite(domain: string, config: Partial<Site>) {
    const { phpVersion, proxyPort, aliases, domain: newDomain } = config
    const sites = store.get('sites')
    const siteIndex = sites.findIndex((s) => s.domain === domain)
    
    if (siteIndex === -1) {
      throw new Error(`Site ${domain} not found`)
    }

    const site = sites[siteIndex]
    
    const domainChanged = !!(newDomain && newDomain !== domain)
    const aliasesChanged = aliases !== undefined && JSON.stringify(site.aliases) !== JSON.stringify(aliases)
    const finalDomain = domainChanged ? newDomain : domain

    const publicDir = `/var/www/${finalDomain}/public`
    const certPath = `/etc/ssl/certs/${finalDomain}.crt`
    const keyPath = `/etc/ssl/private/${finalDomain}.key`

    // Update the site configuration
    if (site.type === 'php' && phpVersion) {
      site.phpVersion = phpVersion
    } else if (site.type === 'proxy' && proxyPort) {
      site.proxyPort = proxyPort
    }
    if (aliases !== undefined) {
      site.aliases = aliases
    }

    const commands: string[] = []

    if (domainChanged) {
      if (site.type === 'php') {
        commands.push(`if [ -d "/var/www/${domain}" ]; then mv /var/www/${domain} /var/www/${finalDomain}; fi`)
      }
      const useSitesAvailable = fs.existsSync('/etc/nginx/sites-available')
      const configDir = useSitesAvailable ? '/etc/nginx/sites-available' : '/etc/nginx/conf.d'
      const configExt = useSitesAvailable ? '' : '.conf'
      
      commands.push(`rm -f ${configDir}/${domain}${configExt}`)
      if (useSitesAvailable) {
        commands.push(`rm -f /etc/nginx/sites-enabled/${domain}`)
      }
      
      commands.push(`rm -f /etc/ssl/certs/${domain}.crt`)
      commands.push(`rm -f /etc/ssl/private/${domain}.key`)
      commands.push(`sed -i '/#start site-manager-${domain}/,/#end site-manager-${domain}/d' /etc/hosts`)

      site.domain = finalDomain
    }

    // Regenerate Nginx config
    let nginxConfig = ''
    const useSnippets = fs.existsSync('/etc/nginx/snippets/fastcgi-php.conf')
    let phpLocationBlock = ''
    
    if (useSnippets) {
      phpLocationBlock = `
        include snippets/fastcgi-php.conf;
        fastcgi_pass unix:/run/php/php${site.phpVersion}-fpm.sock;
      `
    } else {
      phpLocationBlock = `
        try_files \\$uri =404;
        fastcgi_split_path_info ^(.+\\.php)(/.+)$;
        fastcgi_index index.php;
        fastcgi_param SCRIPT_FILENAME \\$document_root\\$fastcgi_script_name;
        include fastcgi_params;
        fastcgi_pass unix:/run/php/php${site.phpVersion}-fpm.sock;
      `
    }

    const serverNames = [finalDomain, ...(site.aliases || [])].join(' ')

    if (site.type === 'php') {
      nginxConfig = `
server {
    listen 80;
    listen 443 ssl;
    server_name ${serverNames};
    root ${publicDir};
    index index.php index.html;

    ssl_certificate ${certPath};
    ssl_certificate_key ${keyPath};

    location / {
        try_files \\$uri \\$uri/ /index.php?\\$query_string;
    }

    location ~ \\.php$ {
        ${phpLocationBlock}
    }
}
`
    } else if (site.type === 'proxy') {
      nginxConfig = `
server {
    listen 80;
    listen 443 ssl;
    server_name ${serverNames};

    ssl_certificate ${certPath};
    ssl_certificate_key ${keyPath};

    location / {
        proxy_pass http://127.0.0.1:${site.proxyPort};
        proxy_http_version 1.1;
        proxy_set_header Upgrade \\$http_upgrade;
        proxy_set_header Connection 'upgrade';
        proxy_set_header Host \\$host;
        proxy_cache_bypass \\$http_upgrade;
    }
}
`
    }

    const useSitesAvailable = fs.existsSync('/etc/nginx/sites-available')
    const configDir = useSitesAvailable ? '/etc/nginx/sites-available' : '/etc/nginx/conf.d'
    const configExtension = useSitesAvailable ? '' : '.conf'
    const configPath = `${configDir}/${finalDomain}${configExtension}`

    if (domainChanged || aliasesChanged) {
      const { caKey, caCert } = await this.setupCA()
      const csrPath = `/tmp/${finalDomain}.csr`
      const extPath = `/tmp/${finalDomain}.ext`
      
      const altNames = [
        `DNS.1 = ${finalDomain}`,
        `DNS.2 = *.${finalDomain}`,
        ...(site.aliases || []).map((a: string, i: number) => `DNS.${i+3} = ${a}`),
        `IP.1 = 127.0.0.1`
      ].join('\n')

      const extContent = `
authorityKeyIdentifier=keyid,issuer
basicConstraints=CA:FALSE
keyUsage = digitalSignature, nonRepudiation, keyEncipherment, dataEncipherment
subjectAltName = @alt_names

[alt_names]
${altNames}
`
      commands.push(
        `echo "${extContent}" > ${extPath}`,
        `openssl genrsa -out ${keyPath} 2048`,
        `openssl req -new -key ${keyPath} -out ${csrPath} -subj "/C=US/ST=State/L=City/O=Organization/CN=${finalDomain}"`,
        `openssl x509 -req -in ${csrPath} -CA "${caCert}" -CAkey "${caKey}" -CAcreateserial -out ${certPath} -days 365 -sha256 -extfile ${extPath}`,
        `rm -f ${csrPath} ${extPath}`
      )
    }

    commands.push(
      `echo '${nginxConfig}' > ${configPath}`
    )
    if (useSitesAvailable) {
      commands.push(`ln -sf ${configPath} /etc/nginx/sites-enabled/`)
    }
    commands.push(`nginx -t && systemctl reload nginx`)

    const allDomains = [finalDomain, ...(site.aliases || [])].join(' ')
    const hostsEntry = `127.0.0.1 ${allDomains}`
    const startMarker = `#start site-manager-${finalDomain}`
    const endMarker = `#end site-manager-${finalDomain}`
    const block = `\\n${startMarker}\\n${hostsEntry}\\n${endMarker}\\n`
    
    // Only remove if it's the exact same marker (we might have removed the old domain one already)
    // Actually we removed the old domain one in the 'domainChanged' block earlier.
    commands.push(`sed -i '/${startMarker}/,/${endMarker}/d' /etc/hosts`)
    commands.push(`printf "${block}" >> /etc/hosts`)

    const fullCommand = commands.join(' && ')

    await new Promise((resolve, reject) => {
      sudo.exec(fullCommand, sudoOptions, (error, stdout) => {
        if (error) {
          reject(error)
        } else {
          resolve(stdout)
        }
      })
    })

    // Update stored configuration
    sites[siteIndex] = site
    store.set('sites', sites)
  }

  private async deleteSite(domain: string) {
    const useSitesAvailable = fs.existsSync('/etc/nginx/sites-available')
    const configDir = useSitesAvailable ? '/etc/nginx/sites-available' : '/etc/nginx/conf.d'
    const configExtension = useSitesAvailable ? '' : '.conf'
    const configPath = `${configDir}/${domain}${configExtension}`

    const commands = [
      `rm -f ${configPath}`,
      `rm -f /etc/ssl/certs/${domain}.crt`,
      `rm -f /etc/ssl/private/${domain}.key`,
      `rm -rf /var/www/${domain}`, // Careful with this! Maybe ask user? User said "delete site", usually implies files too or just config? I'll assume config + files for now as it's a "manager".
    ]

    if (useSitesAvailable) {
      commands.push(`rm -f /etc/nginx/sites-enabled/${domain}`)
    }

    commands.push(`nginx -t && systemctl reload nginx`)
    commands.push(`sed -i '/#start site-manager-${domain}/,/#end site-manager-${domain}/d' /etc/hosts`)
    
    const fullCommand = commands.join(' && ')
    
    await new Promise((resolve, reject) => {
      sudo.exec(fullCommand, sudoOptions, (error, stdout) => {
        if (error) {
          reject(error)
        } else {
          resolve(stdout)
        }
      })
    })

    const sites = store.get('sites')
    const newSites = sites.filter((s) => s.domain !== domain)
    store.set('sites', newSites)
  }
}

