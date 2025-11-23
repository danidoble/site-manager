import { useState, useEffect } from 'react'
import { Plus, Trash2, Edit, ExternalLink, RefreshCw, Loader2, Server, Code, LayoutGrid, List } from 'lucide-react'
import { Button } from './components/ui/Button'
import { Input } from './components/ui/Input'
import { Select } from './components/ui/Select'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from './components/ui/Card'
import { Badge } from './components/ui/Badge'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from './components/ui/Dialog'
import { ThemeToggle } from './components/ThemeToggle'

interface Site {
  domain: string
  type: 'php' | 'proxy'
  phpVersion?: string
  proxyPort?: number
}

interface Dependencies {
  nginx: boolean
  php: boolean
  openssl: boolean
  certutil: boolean
}

type ViewMode = 'grid' | 'list'

function App() {
  const [loading, setLoading] = useState(true)
  const [dependencies, setDependencies] = useState<Dependencies | null>(null)
  const [sites, setSites] = useState<Site[]>([])
  const [phpVersions, setPhpVersions] = useState<string[]>([])
  const [viewMode, setViewMode] = useState<ViewMode>(() => {
    const stored = localStorage.getItem('viewMode')
    return (stored as ViewMode) || 'grid'
  })
  
  const [showCreateModal, setShowCreateModal] = useState(false)
  const [showEditModal, setShowEditModal] = useState(false)
  const [editingSite, setEditingSite] = useState<Site | null>(null)
  const [formError, setFormError] = useState('')
  const [submitting, setSubmitting] = useState(false)
  
  const [formData, setFormData] = useState<Partial<Site>>({
    type: 'php',
    domain: '',
    phpVersion: '',
    proxyPort: 3000
  })

  useEffect(() => {
    checkSystem()
  }, [])

  useEffect(() => {
    localStorage.setItem('viewMode', viewMode)
  }, [viewMode])

  const checkSystem = async () => {
    setLoading(true)
    try {
      const deps = await window.ipcRenderer.invoke('check-dependencies')
      setDependencies(deps)
      
      if (Object.values(deps).every(Boolean)) {
        loadSites()
        loadPhpVersions()
      }
    } catch (e) {
      console.error(e)
    } finally {
      setLoading(false)
    }
  }

  const loadSites = async () => {
    const loadedSites = await window.ipcRenderer.invoke('get-sites')
    setSites(loadedSites || [])
  }

  const loadPhpVersions = async () => {
    const versions = await window.ipcRenderer.invoke('get-php-versions')
    setPhpVersions(versions)
    if (versions.length > 0) {
      setFormData(prev => ({ ...prev, phpVersion: versions[0] }))
    }
  }

  const handleInstallDependencies = async () => {
    setLoading(true)
    try {
      await window.ipcRenderer.invoke('install-dependencies')
      await checkSystem()
    } catch (e) {
      alert('Failed to install dependencies: ' + e)
      setLoading(false)
    }
  }

  const handleCreateSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setFormError('')
    setSubmitting(true)

    try {
      if (!formData.domain) throw new Error('Domain is required')
      if (formData.type === 'php' && !formData.phpVersion) throw new Error('PHP Version is required')
      if (formData.type === 'proxy' && !formData.proxyPort) throw new Error('Port is required')

      await window.ipcRenderer.invoke('create-site', formData)
      await loadSites()
      setShowCreateModal(false)
      setFormData({
        type: 'php',
        domain: '',
        phpVersion: phpVersions[0] || '',
        proxyPort: 3000
      })
    } catch (e: any) {
      setFormError(e.message || 'An error occurred')
    } finally {
      setSubmitting(false)
    }
  }

  const handleEditSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!editingSite) return
    
    setFormError('')
    setSubmitting(true)

    try {
      const updateConfig: any = {}
      if (editingSite.type === 'php') {
        updateConfig.phpVersion = formData.phpVersion
      } else if (editingSite.type === 'proxy') {
        updateConfig.proxyPort = formData.proxyPort
      }

      await window.ipcRenderer.invoke('update-site', editingSite.domain, updateConfig)
      await loadSites()
      setShowEditModal(false)
      setEditingSite(null)
    } catch (e: any) {
      setFormError(e.message || 'An error occurred')
    } finally {
      setSubmitting(false)
    }
  }

  const handleEdit = (site: Site) => {
    setEditingSite(site)
    setFormData({
      type: site.type,
      phpVersion: site.phpVersion,
      proxyPort: site.proxyPort
    })
    setShowEditModal(true)
  }

  const handleDelete = async (domain: string) => {
    if (!confirm(`Are you sure you want to delete ${domain}? This will remove config and files.`)) return
    
    setLoading(true)
    try {
      await window.ipcRenderer.invoke('delete-site', domain)
      await loadSites()
    } catch (e) {
      alert('Failed to delete site: ' + e)
    } finally {
      setLoading(false)
    }
  }

  const handleRegenerateSiteCert = async (domain: string) => {
    setLoading(true)
    try {
      await window.ipcRenderer.invoke('regenerate-site-cert', domain)
      alert(`Certificate for ${domain} regenerated successfully.`)
    } catch (e) {
      alert(`Failed to regenerate certificate for ${domain}: ` + e)
    } finally {
      setLoading(false)
    }
  }

  const toggleViewMode = () => {
    setViewMode(prev => prev === 'grid' ? 'list' : 'grid')
  }

  if (loading) {
    return (
      <div className="flex items-center justify-center min-h-screen">
        <div className="flex flex-col items-center gap-4">
          <Loader2 className="h-8 w-8 animate-spin text-primary" />
          <p className="text-muted-foreground">Loading...</p>
        </div>
      </div>
    )
  }

  if (dependencies && !Object.values(dependencies).every(Boolean)) {
    return (
      <div className="container mx-auto p-8 max-w-2xl">
        <Card className="text-center">
          <CardHeader>
            <CardTitle className="text-2xl">Missing Dependencies</CardTitle>
            <CardDescription>The following components are required:</CardDescription>
          </CardHeader>
          <CardContent>
            <ul className="space-y-2 mb-6">
              {!dependencies.nginx && <li className="text-destructive">❌ Nginx</li>}
              {!dependencies.php && <li className="text-destructive">❌ PHP</li>}
              {!dependencies.openssl && <li className="text-destructive">❌ OpenSSL</li>}
              {!dependencies.certutil && <li className="text-destructive">❌ libnss3-tools (certutil)</li>}
            </ul>
            <Button onClick={handleInstallDependencies} size="lg">
              Install Dependencies (Sudo)
            </Button>
          </CardContent>
        </Card>
      </div>
    )
  }

  return (
    <div className="min-h-screen bg-background">
      <div className="container mx-auto p-6 max-w-7xl">
        {/* Header */}
        <div className="flex items-center justify-between mb-8">
          <div className="flex items-center gap-3">
            <Server className="h-8 w-8 text-primary" />
            <h1 className="text-3xl font-bold">Site Manager</h1>
          </div>
          <div className="flex items-center gap-2">
            <ThemeToggle />
            <Button 
              variant="outline" 
              size="icon"
              onClick={toggleViewMode}
              title={`Switch to ${viewMode === 'grid' ? 'list' : 'grid'} view`}
            >
              {viewMode === 'grid' ? <List className="h-4 w-4" /> : <LayoutGrid className="h-4 w-4" />}
            </Button>
            <Button onClick={() => setShowCreateModal(true)}>
              <Plus className="h-4 w-4 mr-2" />
              New Site
            </Button>
          </div>
        </div>

        {/* Sites Display */}
        {sites.length === 0 ? (
          <Card className="text-center py-12">
            <CardContent>
              <Code className="h-12 w-12 mx-auto mb-4 text-muted-foreground" />
              <h3 className="text-lg font-semibold mb-2">No sites yet</h3>
              <p className="text-muted-foreground mb-4">Create your first site to get started</p>
              <Button onClick={() => setShowCreateModal(true)}>
                <Plus className="h-4 w-4 mr-2" />
                Create Site
              </Button>
            </CardContent>
          </Card>
        ) : viewMode === 'grid' ? (
          /* Grid View */
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
            {sites.map(site => (
              <Card key={site.domain} className="group">
                <CardHeader>
                  <div className="flex items-start justify-between">
                    <div className="flex-1">
                      <CardTitle className="text-lg mb-2">{site.domain}</CardTitle>
                      <div className="flex items-center gap-2 flex-wrap">
                        <Badge variant={site.type === 'php' ? 'default' : 'secondary'}>
                          {site.type.toUpperCase()}
                        </Badge>
                        {site.type === 'php' && site.phpVersion && (
                          <span className="text-xs text-muted-foreground">PHP {site.phpVersion}</span>
                        )}
                        {site.type === 'proxy' && site.proxyPort && (
                          <span className="text-xs text-muted-foreground">Port {site.proxyPort}</span>
                        )}
                      </div>
                    </div>
                  </div>
                </CardHeader>
                <CardContent>
                  <div className="flex items-center gap-2">
                    <Button
                      variant="outline"
                      size="sm"
                      className="flex-1"
                      onClick={() => window.open(`https://${site.domain}`, '_blank')}
                    >
                      <ExternalLink className="h-3 w-3 mr-1" />
                      Open
                    </Button>
                    <Button
                      variant="ghost"
                      size="icon"
                      onClick={() => handleEdit(site)}
                      title="Edit site"
                    >
                      <Edit className="h-4 w-4" />
                    </Button>
                    <Button
                      variant="ghost"
                      size="icon"
                      onClick={() => handleRegenerateSiteCert(site.domain)}
                      title="Regenerate SSL Certificate"
                    >
                      <RefreshCw className="h-4 w-4" />
                    </Button>
                    <Button
                      variant="ghost"
                      size="icon"
                      onClick={() => handleDelete(site.domain)}
                      title="Delete site"
                      className="text-destructive hover:text-destructive"
                    >
                      <Trash2 className="h-4 w-4" />
                    </Button>
                  </div>
                </CardContent>
              </Card>
            ))}
          </div>
        ) : (
          /* List View */
          <div className="border rounded-lg overflow-hidden">
            <table className="w-full">
              <thead className="bg-muted/50">
                <tr>
                  <th className="text-left p-4 font-semibold">Domain</th>
                  <th className="text-left p-4 font-semibold">Type</th>
                  <th className="text-left p-4 font-semibold">Configuration</th>
                  <th className="text-right p-4 font-semibold">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y">
                {sites.map(site => (
                  <tr key={site.domain} className="hover:bg-muted/30 transition-colors">
                    <td className="p-4">
                      <div className="font-medium">{site.domain}</div>
                    </td>
                    <td className="p-4">
                      <Badge variant={site.type === 'php' ? 'default' : 'secondary'}>
                        {site.type.toUpperCase()}
                      </Badge>
                    </td>
                    <td className="p-4">
                      <span className="text-sm text-muted-foreground">
                        {site.type === 'php' && site.phpVersion && `PHP ${site.phpVersion}`}
                        {site.type === 'proxy' && site.proxyPort && `Port ${site.proxyPort}`}
                      </span>
                    </td>
                    <td className="p-4">
                      <div className="flex items-center justify-end gap-1">
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => window.open(`https://${site.domain}`, '_blank')}
                          title="Open site"
                        >
                          <ExternalLink className="h-4 w-4" />
                        </Button>
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => handleEdit(site)}
                          title="Edit site"
                        >
                          <Edit className="h-4 w-4" />
                        </Button>
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => handleRegenerateSiteCert(site.domain)}
                          title="Regenerate SSL Certificate"
                        >
                          <RefreshCw className="h-4 w-4" />
                        </Button>
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => handleDelete(site.domain)}
                          title="Delete site"
                          className="text-destructive hover:text-destructive"
                        >
                          <Trash2 className="h-4 w-4" />
                        </Button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}

        {/* Create Site Modal */}
        <Dialog open={showCreateModal} onOpenChange={setShowCreateModal}>
          <DialogContent onClose={() => setShowCreateModal(false)}>
            <DialogHeader>
              <DialogTitle>Create New Site</DialogTitle>
              <DialogDescription>
                Set up a new development site with Nginx and SSL
              </DialogDescription>
            </DialogHeader>
            <form onSubmit={handleCreateSubmit} className="space-y-4 mt-4">
              <div>
                <label className="text-sm font-medium mb-2 block">Domain Name</label>
                <Input
                  type="text"
                  placeholder="example.local"
                  value={formData.domain}
                  onChange={e => setFormData({...formData, domain: e.target.value})}
                />
              </div>

              <div>
                <label className="text-sm font-medium mb-2 block">Type</label>
                <Select
                  value={formData.type}
                  onChange={e => setFormData({...formData, type: e.target.value as any})}
                >
                  <option value="php">PHP Site</option>
                  <option value="proxy">Node/Proxy App</option>
                </Select>
              </div>

              {formData.type === 'php' && (
                <div>
                  <label className="text-sm font-medium mb-2 block">PHP Version</label>
                  <Select
                    value={formData.phpVersion}
                    onChange={e => setFormData({...formData, phpVersion: e.target.value})}
                  >
                    {phpVersions.map(v => (
                      <option key={v} value={v}>{v}</option>
                    ))}
                  </Select>
                </div>
              )}

              {formData.type === 'proxy' && (
                <div>
                  <label className="text-sm font-medium mb-2 block">Port</label>
                  <Input
                    type="number"
                    value={formData.proxyPort}
                    onChange={e => setFormData({...formData, proxyPort: parseInt(e.target.value)})}
                  />
                </div>
              )}

              {formError && <p className="text-sm text-destructive">{formError}</p>}

              <DialogFooter>
                <Button type="button" variant="outline" onClick={() => setShowCreateModal(false)}>
                  Cancel
                </Button>
                <Button type="submit" disabled={submitting}>
                  {submitting ? (
                    <>
                      <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                      Creating...
                    </>
                  ) : (
                    'Create Site'
                  )}
                </Button>
              </DialogFooter>
            </form>
          </DialogContent>
        </Dialog>

        {/* Edit Site Modal */}
        <Dialog open={showEditModal} onOpenChange={setShowEditModal}>
          <DialogContent onClose={() => setShowEditModal(false)}>
            <DialogHeader>
              <DialogTitle>Edit Site</DialogTitle>
              <DialogDescription>
                Update configuration for {editingSite?.domain}
              </DialogDescription>
            </DialogHeader>
            <form onSubmit={handleEditSubmit} className="space-y-4 mt-4">
              {editingSite?.type === 'php' && (
                <div>
                  <label className="text-sm font-medium mb-2 block">PHP Version</label>
                  <Select
                    value={formData.phpVersion}
                    onChange={e => setFormData({...formData, phpVersion: e.target.value})}
                  >
                    {phpVersions.map(v => (
                      <option key={v} value={v}>{v}</option>
                    ))}
                  </Select>
                </div>
              )}

              {editingSite?.type === 'proxy' && (
                <div>
                  <label className="text-sm font-medium mb-2 block">Port</label>
                  <Input
                    type="number"
                    value={formData.proxyPort}
                    onChange={e => setFormData({...formData, proxyPort: parseInt(e.target.value)})}
                  />
                </div>
              )}

              {formError && <p className="text-sm text-destructive">{formError}</p>}

              <DialogFooter>
                <Button type="button" variant="outline" onClick={() => setShowEditModal(false)}>
                  Cancel
                </Button>
                <Button type="submit" disabled={submitting}>
                  {submitting ? (
                    <>
                      <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                      Updating...
                    </>
                  ) : (
                    'Update Site'
                  )}
                </Button>
              </DialogFooter>
            </form>
          </DialogContent>
        </Dialog>
      </div>
    </div>
  )
}

export default App
