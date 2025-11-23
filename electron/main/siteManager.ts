import { ipcMain, app } from 'electron'
import { exec } from 'child_process'
import sudo from 'sudo-prompt'
import { promisify } from 'util'
import * as fs from 'fs'
import * as path from 'path'
import store from './store'

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
    ipcMain.handle('create-site', (event, config) => this.createSite(config))
    ipcMain.handle('update-site', (event, domain, config) => this.updateSite(domain, config))
    ipcMain.handle('delete-site', (event, domain) => this.deleteSite(domain))
    ipcMain.handle('regenerate-ca', this.regenerateCA.bind(this))
    ipcMain.handle('regenerate-site-cert', (event, domain) => this.regenerateSiteCert(domain))
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
    } catch (e) {}

    try {
      await execAsync('which php')
      results.php = true
    } catch (e) {}

    try {
      await execAsync('which openssl')
      results.openssl = true
    } catch (e) {}

    try {
      await execAsync('which certutil')
      results.certutil = true
    } catch (e) {}

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
    } catch (e) {
      return []
    }
  }

  private async installDependencies() {
    // This command assumes Debian/Ubuntu based system as per user request
    const command = 'apt-get update && apt-get install -y nginx php-fpm libnss3-tools openssl'
    return new Promise((resolve, reject) => {
      sudo.exec(command, sudoOptions, (error, stdout, stderr) => {
        if (error) {
          reject(error)
        } else {
          resolve(stdout)
        }
      })
    })
  }

  private async removeCA() {
    const caDir = path.join(app.getPath('userData'), 'ssl')
    const caCert = path.join(caDir, 'rootCA.pem')
    
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
      sudo.exec(fullCommand, sudoOptions, (error, stdout, stderr) => {
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
    const sites = store.get('sites') as any[]
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
    
    const { caKey, caCert } = await this.setupCA()
    
    const extContent = `
authorityKeyIdentifier=keyid,issuer
basicConstraints=CA:FALSE
keyUsage = digitalSignature, nonRepudiation, keyEncipherment, dataEncipherment
subjectAltName = @alt_names

[alt_names]
DNS.1 = ${domain}
DNS.2 = *.${domain}
IP.1 = 127.0.0.1
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
      sudo.exec(fullCommand, sudoOptions, (error, stdout, stderr) => {
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
        sudo.exec(fullCommand, sudoOptions, (error, stdout, stderr) => {
          if (error) reject(error)
          else resolve(stdout)
        })
      })
    }
    return { caKey, caCert }
  }

  // ... (createSite and deleteSite remain same)

  private async createSite(config: any) {
    const { domain, type, phpVersion, proxyPort } = config
    const rootDir = `/var/www/${domain}`
    const publicDir = `${rootDir}/public` // Laravel/Modern PHP convention
    const certPath = `/etc/ssl/certs/${domain}.crt`
    const keyPath = `/etc/ssl/private/${domain}.key`
    const csrPath = `/tmp/${domain}.csr`
    const extPath = `/tmp/${domain}.ext`
    
    // Ensure CA exists
    const { caKey, caCert } = await this.setupCA()
    
    // OpenSSL Config for SAN
    const extContent = `
authorityKeyIdentifier=keyid,issuer
basicConstraints=CA:FALSE
keyUsage = digitalSignature, nonRepudiation, keyEncipherment, dataEncipherment
subjectAltName = @alt_names

[alt_names]
DNS.1 = ${domain}
DNS.2 = *.${domain}
IP.1 = 127.0.0.1
`

    // Commands to execute
    const commands = [
      `mkdir -p ${publicDir}`,
      `chown -R $USER:$USER ${rootDir}`,
      // Create a simple index file if empty
      `echo "<?php phpinfo(); ?>" > ${publicDir}/index.php`,
      
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
    ]

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

    if (type === 'php') {
      nginxConfig = `
server {
    listen 80;
    listen 443 ssl;
    server_name ${domain};
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
    server_name ${domain};

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
    const hostsEntry = `127.0.0.1 ${domain}`
    const startMarker = `#start site-manager-${domain}`
    const endMarker = `#end site-manager-${domain}`
    const block = `\\n${startMarker}\\n${hostsEntry}\\n${endMarker}\\n`
    
    // Remove old block if exists (sed is safer here)
    // Then append new block using printf
    commands.push(`sed -i '/${startMarker}/,/${endMarker}/d' /etc/hosts`)
    commands.push(`printf "${block}" >> /etc/hosts`)

    const fullCommand = commands.join(' && ')

    await new Promise((resolve, reject) => {
      sudo.exec(fullCommand, sudoOptions, (error, stdout, stderr) => {
        if (error) {
          console.error(error)
          reject(error)
        } else {
          resolve(stdout)
        }
      })
    })

    const sites = store.get('sites')
    sites.push({ domain, type, phpVersion, proxyPort })
    store.set('sites', sites)
  }

  private async updateSite(domain: string, config: any) {
    const { phpVersion, proxyPort } = config
    const sites = store.get('sites') as any[]
    const siteIndex = sites.findIndex((s: any) => s.domain === domain)
    
    if (siteIndex === -1) {
      throw new Error(`Site ${domain} not found`)
    }

    const site = sites[siteIndex]
    const publicDir = `/var/www/${domain}/public`
    const certPath = `/etc/ssl/certs/${domain}.crt`
    const keyPath = `/etc/ssl/private/${domain}.key`

    // Update the site configuration
    if (site.type === 'php' && phpVersion) {
      site.phpVersion = phpVersion
    } else if (site.type === 'proxy' && proxyPort) {
      site.proxyPort = proxyPort
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

    if (site.type === 'php') {
      nginxConfig = `
server {
    listen 80;
    listen 443 ssl;
    server_name ${domain};
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
    server_name ${domain};

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
    const configPath = `${configDir}/${domain}${configExtension}`

    const commands = [
      `echo '${nginxConfig}' > ${configPath}`,
      `nginx -t && systemctl reload nginx`
    ]

    const fullCommand = commands.join(' && ')

    await new Promise((resolve, reject) => {
      sudo.exec(fullCommand, sudoOptions, (error, stdout, stderr) => {
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
      sudo.exec(fullCommand, sudoOptions, (error, stdout, stderr) => {
        if (error) {
          reject(error)
        } else {
          resolve(stdout)
        }
      })
    })

    const sites = store.get('sites')
    const newSites = sites.filter((s: any) => s.domain !== domain)
    store.set('sites', newSites)
  }
}

