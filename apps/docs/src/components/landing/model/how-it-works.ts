/* How it works: the architecture diagram's agents and the two install steps with their commands. */

import { archAgents, type ServerId } from '../mock-data'
import { installCommands, type LandingLang, landingCopy } from '../translations'
import { type CommandSegment, segs } from './shared'

interface HowStep {
  d: string
  /** The step number ("1", "2"), unique. */
  id: string
  n: string
  /** Step 1 installs the server with Docker; step 2 enrolls an agent. */
  segments: CommandSegment[]
  t: string
}

interface HowModel {
  archAgents: { id: ServerId; name: string; os: string }[]
  steps: HowStep[]
}

export function buildHowItWorks(lang: LandingLang): HowModel {
  const commands = [installCommands.docker, installCommands.agent]
  return {
    archAgents,
    steps: landingCopy[lang].how.steps.map((step, index) => ({
      id: step.n,
      ...step,
      segments: segs(commands[index])
    }))
  }
}
