/*
 * Captures the iOS screens the landing page shows, from the iOS Simulator.
 *
 *   bun run capture:ios [simulator-udid]     # in apps/landing; defaults to the booted simulator
 *
 * Run `bun run capture` first: this script reads the same demo server, after that script has renamed its fleet.
 * The simulator needs a Debug build of apps/ios installed, because the launch environment below seeds a signed-in
 * session and only Debug builds read it.
 *
 * Output: src/assets/phones/{name}-{lang}-{theme}.webp, lossless.
 */

import { execFileSync } from 'node:child_process'
import { mkdirSync, mkdtempSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { setTimeout as sleep } from 'node:timers/promises'
import { fileURLToPath } from 'node:url'
import sharp from 'sharp'

const BASE = process.env.SERVERBEE_DEMO_URL ?? 'http://localhost:9527'
const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const PHONES = join(ROOT, 'src/assets/phones')
const BUNDLE = 'com.serverbee.mobile'
const INSTALLATION = 'landing-capture'
/** The page shows phones about 250 CSS pixels wide, so a third of the simulator's 1206 pixels is not needed. */
const WIDTH = 804
/** Time for the app to launch, connect its WebSocket and draw live metrics. */
const SETTLE_MS = 7000

const device = process.argv[2] ?? 'booted'

const LOCALES = { en: ['(en)', 'en_US'], zh: ['(zh-Hans)', 'zh_CN'] } as const
const THEMES = ['light', 'dark'] as const

/** `env` holds the app's DEBUG launch hooks: which tab, deep link or section to open. */
const SCREENS: { env: Record<string, string>; name: string }[] = [
  { name: 'servers', env: { SB_UITEST_TAB: '0' } },
  { name: 'detail', env: { SB_UITEST_DEEPLINK: 'server:dev-demo-07' } },
  { name: 'insights', env: { SB_UITEST_TAB: '2' } }
]

function simctl(args: string[], env: Record<string, string> = {}): void {
  execFileSync('xcrun', ['simctl', ...args], { env: { ...process.env, ...env }, stdio: 'ignore' })
}

interface MobileSession {
  access_token: string
  mobile_session_id: string
  refresh_token: string
  revocation_token: string
  user: { id: string; role: string; username: string }
}

/** Access tokens last fifteen minutes, so every launch signs in again. */
async function signIn(): Promise<MobileSession> {
  const response = await fetch(`${BASE}/api/mobile/auth/login`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({
      username: 'admin',
      password: 'admin123',
      installation_id: INSTALLATION,
      device_name: 'Landing capture'
    })
  })
  if (!response.ok) {
    throw new Error(`POST /api/mobile/auth/login answered ${response.status}`)
  }
  return ((await response.json()) as { data: MobileSession }).data
}

/** simctl passes variables named SIMCTL_CHILD_<NAME> to the launched app as <NAME>. */
function launchEnv(session: MobileSession, screen: Record<string, string>): Record<string, string> {
  const app: Record<string, string> = {
    SB_UITEST_SERVER: BASE,
    SB_UITEST_ACCESS: session.access_token,
    SB_UITEST_REFRESH: session.refresh_token,
    SB_UITEST_USER_ID: session.user.id,
    SB_UITEST_USERNAME: session.user.username,
    SB_UITEST_ROLE: session.user.role,
    SB_UITEST_INSTALLATION_ID: INSTALLATION,
    SB_UITEST_REVOCATION: session.revocation_token,
    SB_UITEST_MOBILE_SESSION_ID: session.mobile_session_id,
    ...screen
  }
  return Object.fromEntries(Object.entries(app).map(([name, value]) => [`SIMCTL_CHILD_${name}`, value]))
}

async function main() {
  mkdirSync(PHONES, { recursive: true })
  const scratch = mkdtempSync(join(tmpdir(), 'serverbee-phones-'))
  simctl(['status_bar', device, 'override', '--time', '9:41', '--batteryState', 'charged', '--batteryLevel', '100'])
  try {
    for (const screen of SCREENS) {
      for (const [lang, [languages, locale]] of Object.entries(LOCALES)) {
        for (const theme of THEMES) {
          simctl(['ui', device, 'appearance', theme])
          try {
            simctl(['terminate', device, BUNDLE])
          } catch {
            // The app was not running.
          }
          simctl(
            ['launch', device, BUNDLE, '-AppleLanguages', languages, '-AppleLocale', locale],
            launchEnv(await signIn(), screen.env)
          )
          await sleep(SETTLE_MS)
          const png = join(scratch, `${screen.name}-${lang}-${theme}.png`)
          simctl(['io', device, 'screenshot', png])
          await sharp(png)
            .resize({ width: WIDTH })
            .webp({ lossless: true, effort: 6 })
            .toFile(join(PHONES, `${screen.name}-${lang}-${theme}.webp`))
          console.log(`phone ${screen.name}-${lang}-${theme}`)
        }
      }
    }
  } finally {
    simctl(['status_bar', device, 'clear'])
    simctl(['ui', device, 'appearance', 'light'])
  }
}

await main()
