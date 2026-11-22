/*
 * The page's motion. Every part looks for its own elements and does nothing without them, so the same runtime
 * serves the landing pages and the pages that have none of this.
 */

import { mountDashboard } from './dashboard'
import { mountFlow } from './flow'
import { mountGrant } from './grant'
import { mountHero } from './hero'
import { mountIpQuality } from './ip-quality'
import { mountKit } from './kit'
import { mountNetwork } from './network'
import { mountPush } from './push'
import { mountRail } from './rail'
import { mountReveal } from './reveal'
import { reduced } from './stage'

export function mountMotion(): void {
  // These two also have a settled form: the routes drawn, and a rail that follows the scroll.
  mountHero()
  mountRail()
  if (reduced) {
    return
  }
  mountReveal()
  mountNetwork()
  mountIpQuality()
  mountGrant()
  mountDashboard()
  mountPush()
  mountKit()
  mountFlow()
}
