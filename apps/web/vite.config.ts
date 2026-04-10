import path from 'node:path'
import tailwindcss from '@tailwindcss/vite'
import { TanStackRouterVite } from '@tanstack/router-plugin/vite'
import react from '@vitejs/plugin-react'
import { defineConfig, loadEnv } from 'vite'
import { VitePWA } from 'vite-plugin-pwa'
import { createDevProxy } from './vite/dev-proxy'

const apiRuntimePattern = /^\/api\//
const pwaRuntimePattern = /^\/pwa-/

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

  if (mode === 'prod-proxy') {
    const env = loadEnv(mode, repoRoot, '')
    const target = requireProdProxyEnv(env, 'SERVERBEE_PROD_URL')
    const readonlyApiKey = requireProdProxyEnv(env, 'SERVERBEE_PROD_READONLY_API_KEY')
    const allowWrites = env.ALLOW_WRITES === '1'

    return {
      plugins: [
        TanStackRouterVite({
          routeFileIgnorePattern: 'components|hooks|types\\.ts'
        }),
        react(),
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
          '@': path.resolve(import.meta.dirname, './src')
        }
      },
      build: {
        rollupOptions: {
          output: {
            manualChunks: {
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
        routeFileIgnorePattern: 'components|hooks|types\\.ts'
      }),
      react(),
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
        '@': path.resolve(import.meta.dirname, './src')
      }
    },
    build: {
      rollupOptions: {
        output: {
          manualChunks: {
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
