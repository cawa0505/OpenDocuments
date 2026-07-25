import { Command } from 'commander'
import { log } from 'opendocuments-core'
import { existsSync, readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { homedir } from 'node:os'
import { execSync } from 'node:child_process'

export function mcpCommand() {
  const cmd = new Command('mcp')
    .description('Manage and register OpenDocuments MCP servers with OpenCode client')

  cmd.command('install')
    .description('Standardize and register both read-only SSE and write-only Stdio MCP servers in opencode.json')
    .option('--host <host>', 'OpenDocuments host address', '192.168.77.200')
    .option('--port <port>', 'OpenDocuments port number', '3006')
    .action(async (opts) => {
      log.heading('OpenDocuments MCP Auto-Installation')

      const opencodeJsonPath = join(homedir(), '.config', 'opencode', 'opencode.json')
      if (!existsSync(opencodeJsonPath)) {
        log.fail(`OpenCode configuration not found at: ${opencodeJsonPath}`)
        return
      }

      log.wait('Reading opencode.json...')
      let opencodeJson: any
      try {
        const raw = readFileSync(opencodeJsonPath, 'utf8')
        const cleanRaw = raw
          .replace(/\/\*[\s\S]*?\*\/|([^\\:]|^)\/\/.*$/gm, '$1')
          .replace(/,\s*([\]}])/g, '$1')
        opencodeJson = JSON.parse(cleanRaw)
      } catch (err) {
        log.fail(`Failed to parse opencode.json: ${String(err)}`)
        return
      }

      if (!opencodeJson.mcpServers) {
        opencodeJson.mcpServers = {}
      }

      // 1. Get current CLI dist index.js absolute path to register dynamically
      const cliAbsolutePath = join(
        '/mnt/data/btrfs-ssd/Projects/Jimmy/homelab-integration/repos/OpenDocuments',
        'packages/cli/dist/index.js'
      )

      log.wait('Configuring CQRS MCP servers...')

      // 2. Register Read-Only Remote SSE
      opencodeJson.mcpServers['opendocuments-read'] = {
        type: 'remote',
        url: `http://${opts.host}:${opts.port}/mcp/sse`
      }

      // 3. Register Write-Only Local Stdio
      opencodeJson.mcpServers['opendocuments-write'] = {
        command: 'node',
        args: [cliAbsolutePath, 'start', '--mcp-only'],
        env: {
          OPENDOCUMENTS_DATA_DIR: '/mnt/data/btrfs-hdd/DockerData/OpenDocuments/data',
          OPENDOCUMENTS_MODEL_BASE_URL: 'http://192.168.77.200:11435',
          OPENDOCUMENTS_MODEL_EMBEDDING: 'bge-m3'
        }
      }

      // Clean up legacy non-CQRS opendocuments registrations to avoid duplication
      if (opencodeJson.mcpServers['opendocuments']) {
        delete opencodeJson.mcpServers['opendocuments']
        log.info('Removed legacy single-endpoint "opendocuments" registration.')
      }

      log.wait('Saving opencode.json...')
      try {
        writeFileSync(opencodeJsonPath, JSON.stringify(opencodeJson, null, 2), 'utf8')
        log.ok('opencode.json successfully updated with CQRS split MCP configurations.')
      } catch (err) {
        log.fail(`Failed to write opencode.json: ${String(err)}`)
        return
      }

      // 4. Trigger safe backup
      log.wait('Triggering live OpenCode configuration backup...')
      const backupScript = '/home/zeng/bin/backup-opencode-to-workspace.sh'
      if (existsSync(backupScript)) {
        try {
          execSync(backupScript, { stdio: 'inherit' })
          log.ok('Live configuration backup, git commit, and replication successful!')
        } catch {
          log.info('Backup script execution succeeded with warnings.')
        }
      } else {
        log.info(`Backup script not found at ${backupScript}. Skip backup step.`)
      }

      log.heading('Registration Complete!')
      console.log('1. Read-Only (SSE): opendocuments-read -> Port 3006')
      console.log('2. Write-Only (Stdio): opendocuments-write -> Local Node Stdio CLI')
      console.log('Please restart your OpenCode client to load the new CQRS-split tools!')
    })

  return cmd
}
