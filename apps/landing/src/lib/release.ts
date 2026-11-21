/* Facts read from the workspace at build time, so the page cannot drift from the release it announces. */

import cargoToml from '../../../../Cargo.toml?raw'
import iosProject from '../../../ios/project.yml?raw'

function read(source: string, pattern: RegExp, what: string): string {
  const match = pattern.exec(source)
  if (!match) {
    throw new Error(`landing: could not read ${what}`)
  }
  return match[1]
}

/** The workspace version in Cargo.toml: the release the page announces. */
export const VERSION = read(
  cargoToml,
  /^\[workspace\.package\]\n(?:(?!\[).*\n)*?version = "([^"]+)"/m,
  'the workspace version from Cargo.toml'
)

/** The major iOS version the app's deployment target requires. */
export const IOS_MIN = read(
  iosProject,
  /deploymentTarget:\s*\n\s*iOS:\s*"(\d+)/,
  'the iOS deployment target from apps/ios/project.yml'
)

/** Replaces the `{version}` and `{ios}` placeholders the copy uses. */
export function fill(text: string): string {
  return text.replaceAll('{version}', VERSION).replaceAll('{ios}', IOS_MIN)
}
