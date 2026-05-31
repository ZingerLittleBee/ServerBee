export const INSTALL_COMMAND =
  'curl -fsSL https://raw.githubusercontent.com/ZingerLittleBee/ServerBee/main/deploy/install.sh | sudo bash -s -- server'

export type LandingLang = 'en' | 'zh'

export const translations = {
  en: {
    hero: {
      eyebrow: 'Open source · MIT · Built with Rust',
      headline1: 'Self-hosted VPS monitoring,',
      headline2: 'in a single binary.',
      sub: 'ServerBee is a lightweight probe that streams CPU, memory, disk, network, and Docker metrics to a Rust dashboard in real time — no agents to babysit, no external database, no bloat.',
      primaryCta: 'Quick start',
      secondaryCta: 'View on GitHub',
      installLabel: 'Install on Linux'
    },
    trust: {
      binary: 'Single binary · ~10 MB',
      realtime: 'Sub-second WebSocket updates',
      deps: 'Zero external dependencies'
    },
    pillars: {
      one: {
        title: 'Lightweight Rust probe',
        body: 'A statically linked binary you drop on any Linux host. No JVM, no Python, no daemons — just a tiny process that idles near zero CPU.'
      },
      two: {
        title: 'Realtime over WebSocket',
        body: 'Agents stream metrics and events to the server with sub-second latency. The browser dashboard subscribes to the same fan-out, so what you see is what is actually happening.'
      },
      three: {
        title: 'One agent, full control',
        body: 'Terminal sessions, file manager, Docker operations, and remote command execution all run through the same encrypted channel — gated by per-server capabilities.'
      }
    },
    bento: {
      network: {
        title: 'Network quality monitoring',
        body: 'ICMP / TCP / HTTP probes, traceroute, packet loss and latency charts, CSV export, and preset targets — all from your agents.'
      },
      terminal: {
        title: 'Browser web terminal',
        body: 'Full PTY sessions over WebSocket. Multi-tab, copy-paste friendly, and audit-logged.'
      },
      file: {
        title: 'File manager',
        body: 'Browse, read, edit, upload and download files through path-sandboxed agents.'
      },
      docker: {
        title: 'Docker management',
        body: 'Containers, stats, events, logs, networks, volumes — when the capability is enabled.'
      },
      themes: {
        title: 'Custom dashboards & themes',
        body: 'Compose dashboards from widgets. Bring your own OKLCH palette, logo, favicon, and footer.'
      },
      alerts: {
        title: 'Alerts & notifications',
        body: 'Thresholds, debounce, maintenance windows. Delivered through Webhook, Telegram, Bark, Email and APNs.'
      },
      monitors: {
        title: 'Service monitors',
        body: 'SSL, DNS, HTTP keyword, TCP and WHOIS checks with history and notifications.'
      },
      upgrade: {
        title: 'Automatic upgrades',
        body: 'Server and agents update themselves. One CLI command, zero downtime restarts.'
      }
    },
    how: {
      title: 'Three commands to a live dashboard',
      step1: {
        title: 'Install the server',
        body: 'Run the install script on one host. Systemd takes over from there.'
      },
      step2: {
        title: 'Bootstrap an agent',
        body: 'Drop the agent binary on every VPS you want to monitor. Pair it once.'
      },
      step3: { title: 'Open the dashboard', body: 'Sign in, watch metrics stream in, and start composing dashboards.' }
    },
    finalCta: {
      title: 'Ship a monitor in five minutes.',
      sub: 'Open source, MIT licensed, and small enough to forget about.',
      readDocs: 'Read the docs',
      star: 'Star on GitHub'
    }
  },
  zh: {
    hero: {
      eyebrow: '开源 · MIT · Rust 构建',
      headline1: '自托管的 VPS 监控，',
      headline2: '只需一个二进制。',
      sub: 'ServerBee 是一个轻量探针：实时把 CPU、内存、磁盘、网络、Docker 指标推送到 Rust 仪表盘，没有外部依赖、没有冗余服务、不用伺候它。',
      primaryCta: '快速开始',
      secondaryCta: '在 GitHub 查看',
      installLabel: 'Linux 一键安装'
    },
    trust: {
      binary: '单二进制 · 约 10 MB',
      realtime: '亚秒级 WebSocket 更新',
      deps: '零外部依赖'
    },
    pillars: {
      one: {
        title: 'Rust 编写的轻量探针',
        body: '静态链接的二进制文件，丢到任何 Linux 主机上就能跑。没有 JVM、没有 Python、没有守护脚本，空载几乎不占 CPU。'
      },
      two: {
        title: 'WebSocket 实时通信',
        body: 'Agent 把指标和事件以亚秒级延迟推到 Server。浏览器订阅同一份广播流，看到的就是当下真实发生的事。'
      },
      three: {
        title: '一个 Agent 全栈掌控',
        body: '终端会话、文件管理、Docker 操作、远程命令执行都走同一条加密通道，并由每台服务器的能力位精细授权。'
      }
    },
    bento: {
      network: {
        title: '网络质量监控',
        body: 'ICMP / TCP / HTTP 探测、traceroute、丢包与延迟图表、CSV 导出、预设目标 —— 全部由你的 Agent 完成。'
      },
      terminal: {
        title: '浏览器终端',
        body: '基于 WebSocket 的完整 PTY 会话，多标签、易复制粘贴、全程审计日志。'
      },
      file: {
        title: '文件管理',
        body: '通过路径沙箱化的 Agent 浏览、读取、编辑、上传和下载文件。'
      },
      docker: {
        title: 'Docker 管理',
        body: '在开启能力位后管理容器、统计、事件、日志、网络与卷。'
      },
      themes: {
        title: '自定义仪表盘与主题',
        body: '从组件拼装仪表盘，自带 OKLCH 调色板、Logo、favicon 和页脚配置。'
      },
      alerts: {
        title: '告警与通知',
        body: '阈值、抖动抑制、维护窗口。通过 Webhook、Telegram、Bark、Email 和 APNs 推送。'
      },
      monitors: {
        title: '服务监控',
        body: 'SSL、DNS、HTTP 关键字、TCP 与 WHOIS 检查，含历史与通知。'
      },
      upgrade: {
        title: '自动升级',
        body: 'Server 和 Agent 自动更新。一条 CLI 命令，重启零停顿。'
      }
    },
    how: {
      title: '三步上线一个实时仪表盘',
      step1: { title: '安装 Server', body: '在一台主机上跑安装脚本，剩下交给 systemd。' },
      step2: { title: '部署 Agent', body: '把 Agent 二进制丢到每台要监控的 VPS 上，配对一次即可。' },
      step3: { title: '打开仪表盘', body: '登录，等指标自动流入，然后开始拼装你的仪表盘。' }
    },
    finalCta: {
      title: '五分钟跑起一个监控。',
      sub: '开源、MIT 协议，小到你会忘了它在跑。',
      readDocs: '阅读文档',
      star: '在 GitHub 上 Star'
    }
  }
} as const

export type Translations = typeof translations
export function t(lang: string): Translations['en'] {
  const table = translations as unknown as Record<string, Translations['en']>
  return table[lang] ?? translations.en
}
