import path from 'node:path'
import tailwindcss from '@tailwindcss/vite'
import { TanStackRouterVite } from '@tanstack/router-plugin/vite'
import react from '@vitejs/plugin-react'
import { boneyardPlugin } from 'boneyard-js/vite'
import { defineConfig, loadEnv } from 'vite'
import { VitePWA } from 'vite-plugin-pwa'
import { createDevProxy } from './vite/dev-proxy'
import { builtinWidgetsPlugin } from './vite-plugins/builtin-widgets'

const apiRuntimePattern = /^\/api\//
const pwaRuntimePattern = /^\/pwa-/
const routeFileIgnorePattern =
  'components|hooks|types\\.ts|\\.test\\.(ts|tsx)$|-page\\.tsx$|-(form|dialog|section|tab|list|chart|item|toggle|buttons|options|payloads|search|utils)\\.tsx?$'

function requireProdProxyEnv(env: Record<string, string>, name: string) {
  const value = env[name]?.trim()

  if (!value) {
    if (name === 'SERVERBEE_PROD_READONLY_API_KEY') {
      throw new Error(
        `Missing required ${name} in the repo root .env for prod-proxy mode. Do not reuse SERVERBEE_PROD_API_KEY or any admin key here, use a dedicated read-only API key.`
      )
    }

    throw new Error(`Missing required ${name} in the repo root .env for prod-proxy mode.`)
  }

  return value
}

export default defineConfig(({ mode }) => {
  const repoRoot = path.resolve(import.meta.dirname, '../..')

  // Boneyard auto-captures bones from the fixture-only /boneyard-capture
  // route when a dev server starts and on HMR (the plugin is apply: 'serve',
  // so builds are unaffected). Capture visits only the local dev server, so
  // prod-proxy mode keeps its safety model — no production data is involved.
  // `bun run generate:bones` drives the CLI against its own ephemeral server
  // and sets BONEYARD_SKIP_PLUGIN=1 so the plugin's auto-capture cannot race
  // the CLI's committed output there.
  const boneyardPlugins = process.env.BONEYARD_SKIP_PLUGIN === '1' ? [] : [boneyardPlugin()]

  if (mode === 'prod-proxy') {
    const env = loadEnv(mode, repoRoot, '')
    const target = requireProdProxyEnv(env, 'SERVERBEE_PROD_URL')
    const readonlyApiKey = requireProdProxyEnv(env, 'SERVERBEE_PROD_READONLY_API_KEY')
    const allowWrites = env.ALLOW_WRITES === '1'

    return {
      plugins: [
        TanStackRouterVite({
          routeFileIgnorePattern,
          autoCodeSplitting: true
        }),
        react(),
        ...boneyardPlugins,
        tailwindcss(),
        VitePWA({
          registerType: 'autoUpdate',
          manifest: {
            name: 'ServerBee',
            short_name: 'ServerBee',
            description: 'Server Monitoring Dashboard',
            start_url: '/',
            display: 'standalone',
            background_color: '#0a0a0a',
            theme_color: '#f59e0b',
            icons: [
              { src: '/pwa-192.png', sizes: '192x192', type: 'image/png' },
              { src: '/pwa-512.png', sizes: '512x512', type: 'image/png' },
              { src: '/pwa-maskable-512.png', sizes: '512x512', type: 'image/png', purpose: 'maskable' }
            ]
          },
          workbox: {
            globPatterns: ['**/*.{js,css,html,woff2,png,svg}'],
            navigateFallback: '/index.html',
            runtimeCaching: [
              { urlPattern: apiRuntimePattern, handler: 'NetworkOnly' },
              { urlPattern: pwaRuntimePattern, handler: 'CacheFirst', options: { cacheName: 'pwa-icons' } }
            ]
          }
        })
      ],
      resolve: {
        alias: {
          '@': path.resolve(import.meta.dirname, './src'),
          '@serverbee/widget-sdk': path.resolve(import.meta.dirname, '../../packages/widget-sdk/src/index.ts'),
          '@serverbee/widget-sdk/z': path.resolve(import.meta.dirname, '../../packages/widget-sdk/src/z/index.ts'),
          '@serverbee/widget-sdk/hooks': path.resolve(
            import.meta.dirname,
            '../../packages/widget-sdk/src/hooks/index.ts'
          ),
          '@serverbee/widget-sdk/actions': path.resolve(
            import.meta.dirname,
            '../../packages/widget-sdk/src/actions/index.ts'
          )
        }
      },
      build: {
        rollupOptions: {
          output: {
            manualChunks: {
              tanstack: ['@tanstack/react-router', '@tanstack/react-query', '@tanstack/react-table'],
              xterm: ['@xterm/xterm', '@xterm/addon-fit', '@xterm/addon-web-links'],
              recharts: ['recharts']
            }
          }
        }
      },
      define: {
        'import.meta.env.VITE_DEV_PROXY_ALLOW_WRITES': JSON.stringify(allowWrites ? '1' : '0'),
        'import.meta.env.VITE_DEV_PROXY_TARGET': JSON.stringify(target)
      },
      server: {
        proxy: {
          '/api': createDevProxy({
            target,
            readonlyApiKey,
            allowWrites
          })
        }
      }
    }
  }

  return {
    plugins: [
      TanStackRouterVite({
        routeFileIgnorePattern,
        autoCodeSplitting: true
      }),
      react(),
      ...boneyardPlugins,
      builtinWidgetsPlugin(),
      tailwindcss(),
      VitePWA({
        registerType: 'autoUpdate',
        manifest: {
          name: 'ServerBee',
          short_name: 'ServerBee',
          description: 'Server Monitoring Dashboard',
          start_url: '/',
          display: 'standalone',
          background_color: '#0a0a0a',
          theme_color: '#f59e0b',
          icons: [
            { src: '/pwa-192.png', sizes: '192x192', type: 'image/png' },
            { src: '/pwa-512.png', sizes: '512x512', type: 'image/png' },
            { src: '/pwa-maskable-512.png', sizes: '512x512', type: 'image/png', purpose: 'maskable' }
          ]
        },
        workbox: {
          globPatterns: ['**/*.{js,css,html,woff2,png,svg}'],
          navigateFallback: '/index.html',
          runtimeCaching: [
            { urlPattern: apiRuntimePattern, handler: 'NetworkOnly' },
            { urlPattern: pwaRuntimePattern, handler: 'CacheFirst', options: { cacheName: 'pwa-icons' } }
          ]
        }
      })
    ],
    resolve: {
      alias: {
        '@': path.resolve(import.meta.dirname, './src'),
        '@serverbee/widget-sdk': path.resolve(import.meta.dirname, '../../packages/widget-sdk/src/index.ts'),
        '@serverbee/widget-sdk/z': path.resolve(import.meta.dirname, '../../packages/widget-sdk/src/z/index.ts'),
        '@serverbee/widget-sdk/hooks': path.resolve(
          import.meta.dirname,
          '../../packages/widget-sdk/src/hooks/index.ts'
        ),
        '@serverbee/widget-sdk/actions': path.resolve(
          import.meta.dirname,
          '../../packages/widget-sdk/src/actions/index.ts'
        )
      }
    },
    build: {
      rollupOptions: {
        output: {
          manualChunks: {
            tanstack: ['@tanstack/react-router', '@tanstack/react-query', '@tanstack/react-table'],
            xterm: ['@xterm/xterm', '@xterm/addon-fit', '@xterm/addon-web-links'],
            recharts: ['recharts']
          }
        }
      }
    },
    server: {
      proxy: {
        '/api': {
          target: 'http://localhost:9527',
          changeOrigin: true,
          ws: true
        }
      }
    }
  }
})
