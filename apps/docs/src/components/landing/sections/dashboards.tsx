import { Points, SectionHead } from '../chrome/copy-blocks'
import { Icon } from '../icon'
import { GridLine, PlotLines } from '../mocks/plot'
import { buildDashboard, type DashGauge } from '../model/dashboard'
import { type LandingLang, landingCopy } from '../translations'

interface DashboardsSectionProps {
  lang: LandingLang
  /** Shared clock for the live widgets; 0 during SSR and the first client render. */
  tick: number
}

/** Edit-mode toolbar of a widget: lock, edit, delete. The top-N widget is the locked one. */
function WidgetChip({ locked = false }: { locked?: boolean }) {
  return (
    <span className="dw-chip">
      <Icon name={locked ? 'lock' : 'lock-open'} />
      <Icon name="pencil" />
      <Icon className="del" name="trash-2" />
    </span>
  )
}

/** A gauge widget; `className` places it in the grid (`g-ga` / `g-gb`). */
function GaugeWidget({ className, gauge }: { className: string; gauge: DashGauge }) {
  return (
    <div className={className}>
      <div className="gauge" style={gauge.style}>
        <div className="gauge-in">
          <Icon name={gauge.icon} />
          <span>{gauge.label}</span>
          <b>
            {gauge.val}
            <small>%</small>
          </b>
        </div>
      </div>
      <span className="dw-s">{gauge.server}</span>
      <WidgetChip />
      <span className="dw-rs" />
    </div>
  )
}

export function DashboardsSection({ lang, tick }: DashboardsSectionProps) {
  const copy = landingCopy[lang]
  const dash = copy.dash
  const p = copy.p
  const w = p.w
  const model = buildDashboard(lang, tick)
  return (
    <section className="sec" id="dash">
      <div className="shell">
        <SectionHead copy={dash} />
        <div aria-hidden="true" className="panel db pm">
          <div className="db-top">
            <Icon name="panel-left" />
            <span className="db-switch">
              <span>{p.dashName}</span>
              <Icon name="chevron-down" />
            </span>
            <span className="db-acts">
              <span className="db-btn">
                <Icon name="plus" />
                {p.addWidget}
              </span>
              <span className="db-btn">{p.cancel}</span>
              <span className="db-btn pri">{p.save}</span>
            </span>
          </div>
          <div className="db-canvas">
            <div className="db-grid">
              {model.stats.map((stat) => (
                <div className={stat.className} key={stat.id}>
                  <span className="db-ico">
                    <Icon name={stat.icon} />
                  </span>
                  <span className="dws">
                    <span className="dws-l">{stat.label}</span>
                    <span className="dws-v">{stat.val}</span>
                    <span className="dws-s">{stat.sub}</span>
                  </span>
                  <WidgetChip />
                </div>
              ))}

              <div className="dw g-cmp">
                <span className="dw-t">{w.cpuCmp}</span>
                <div className="plot">
                  <GridLine label="50" top="0%" />
                  <GridLine label="25" top="50%" />
                  <GridLine base label="0" top="100%" />
                  <PlotLines lines={model.cmp} />
                </div>
                <div className="lgd">
                  {model.cmp.map((line) => (
                    <span key={line.id}>
                      <i style={line.dotStyle} />
                      {line.name}
                    </span>
                  ))}
                </div>
                <WidgetChip />
                <span className="dw-rs" />
              </div>

              <GaugeWidget className="dw dw-gauge g-ga" gauge={model.gaugeA} />
              <GaugeWidget className="dw dw-gauge g-gb" gauge={model.gaugeB} />

              <div className="dw g-top">
                <span className="dw-t">{w.topMem}</span>
                <div className="tn">
                  {model.top.map((row) => (
                    <div className="tn-bar" key={row.id} style={row.style}>
                      {row.name}
                    </div>
                  ))}
                </div>
                <div className="tn-axis">
                  <span>0%</span>
                  <span>25%</span>
                  <span>50%</span>
                  <span>75%</span>
                  <span>100%</span>
                </div>
                <WidgetChip locked />
              </div>

              <div className="dw g-up">
                <span className="dw-t">
                  {w.uptime} <span className="dw-s">{w.uptimeSub}</span>
                </span>
                <div className="up-rows">
                  {model.uptime.map((row) => (
                    <div className="up-row" key={row.id}>
                      <span>{row.name}</span>
                      <div className="up-cells">
                        {row.cells.map((cell) => (
                          <i className={cell.className} key={cell.id} />
                        ))}
                      </div>
                      <b>{row.pct}</b>
                    </div>
                  ))}
                </div>
                <WidgetChip />
                <span className="dw-rs" />
              </div>

              <div className="dw g-trf">
                <span className="dw-t">
                  {w.traffic} <span className="dw-s">{w.trafficSub}</span>
                </span>
                <div className="tb-plot">
                  {model.bars.map((bar) => (
                    <div className="tb-col" key={bar.id}>
                      <div className="tb-out" style={bar.outStyle} />
                      <div className="tb-in" style={bar.inStyle} />
                    </div>
                  ))}
                </div>
                <div className="tb-lab">
                  {model.bars.map((bar) => (
                    <span key={bar.id}>{bar.label}</span>
                  ))}
                </div>
                <WidgetChip />
                <span className="dw-rs" />
              </div>

              <div className="db-ph g-ph" />
              <div className="dw db-drag g-ph">
                <span className="dw-t">{w.md}</span>
                <div className="md-body">
                  {model.mdLines.map((line) => (
                    <span key={line.id}>{line.text}</span>
                  ))}
                </div>
                <WidgetChip />
                <span className="dw-rs" />
              </div>
            </div>
          </div>
        </div>
        <Points className="pts-row" points={dash.points} />
      </div>
    </section>
  )
}
