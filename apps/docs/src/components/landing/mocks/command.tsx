import { Fragment } from 'react'

import type { CommandSegment } from '../model/shared'

interface CommandSegmentsProps {
  /** From the model (`segs(command)`): install, grant or step commands. */
  segments: readonly CommandSegment[]
}

/**
 * A shell command as `<span class="cmd-seg">` segments with no wrapper element. Each segment is followed
 * by its gap as plain text (the space after a word) or, inside a word and at the end, by `<wbr />`: lines
 * break only there, and selecting the text copies the exact command. The segments are inline-blocks (see
 * .cmd-seg in landing.css); role none keeps them out of the accessibility tree, so the command still reads
 * as one run of text.
 */
export function CommandSegments({ segments }: CommandSegmentsProps) {
  return segments.map((segment) => (
    <Fragment key={segment.id}>
      <span className="cmd-seg" role="none">
        {segment.text}
      </span>
      {segment.gap || <wbr />}
    </Fragment>
  ))
}
