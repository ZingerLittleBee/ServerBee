import type { ReactNode } from 'react'

import { Icon } from '../icon'
import { buildIosTabs } from '../model/ios'
import type { LandingLang } from '../translations'

interface PhoneProps {
  /** The screen content, placed inside `.ios-content`. */
  children: ReactNode
  lang: LandingLang
  /** Rendered between the status bar and the content, e.g. the `.ios-push` banner. */
  overlay?: ReactNode
  /** `sm`: the hero phone (scale 0.5). `md`: the iOS section phones (scale 0.62). */
  size: 'sm' | 'md'
}

/**
 * An iPhone frame: island, 9:41 status bar, the screen content and the tab bar (Servers selected,
 * one alert). The `<i>` elements in the signal and battery indicators are shapes, not icons.
 */
export function Phone({ lang, size, overlay, children }: PhoneProps) {
  return (
    <div className={`ph-wrap ph-${size}`}>
      <div className="phone">
        <div className="ph-screen">
          <div className="ph-island" />
          <div className="ios-status">
            <span>9:41</span>
            <span className="ios-sys">
              <span className="ios-sig">
                <i />
                <i />
                <i />
                <i />
              </span>
              <Icon className="ios-wifi" name="wifi" />
              <span className="ios-bat">
                <i />
              </span>
            </span>
          </div>
          {overlay}
          <div className="ios-content">{children}</div>
          <div className="ios-tabbar">
            {buildIosTabs(lang).map((tab) => (
              <span className={tab.className} key={tab.id}>
                <Icon name={tab.icon} />
                {tab.label}
                {tab.badge ? <em className="ios-badge-n">1</em> : null}
              </span>
            ))}
          </div>
        </div>
      </div>
    </div>
  )
}
