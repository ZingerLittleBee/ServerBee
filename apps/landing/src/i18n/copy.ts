/* Bilingual landing copy. fill() in src/lib/release.ts replaces `{version}` and `{ios}` when a page renders. */

import { docsUrl, type Lang, links } from '@/lib/site'

export type InstallTab = 'docker' | 'binary' | 'railway'

/** The product captures in src/assets/shots, by file name prefix. */
export type ShotName = 'capabilities' | 'dashboard' | 'fleet' | 'ip-quality' | 'network'

/** The iOS captures in src/assets/phones, by file name prefix. */
export type PhoneName = 'detail' | 'insights' | 'servers'

/** `href` is an in-page `#section` anchor or an absolute URL. */
export interface LinkCopy {
  href: string
  label: string
}

/** A titled point: `t` is the title, `d` the description. */
export interface PointCopy {
  d: string
  t: string
}

export interface FeatureCopy {
  h2: string
  lede: string
  link: LinkCopy
  points: PointCopy[]
}

interface NavCopy {
  cta: string
  githubLabel: string
  /** Accessible name of the header navigation landmark. */
  label: string
  /** The other language, named in that language. */
  langBtn: string
  links: LinkCopy[]
  skip: string
  themeLabel: string
}

interface HeroCopy {
  copied: string
  /** Announced to screen readers after a successful copy. */
  copiedStatus: string
  copy: string
  /** Copy button label when the clipboard write fails; the command is selected instead. */
  copyFailed: string
  /** Announced to screen readers when the clipboard write fails. */
  copyFailedStatus: string
  cta2: string
  facts: string[]
  h1a: string
  h1b: string
  /** Accessible name of the install method tabs. */
  installLabel: string
  installNote: string
  railwayBtn: string
  railwayText: string
  release: string
  releaseLink: string
  sub: string
  tabs: Record<InstallTab, string>
}

interface SecurityCopy extends FeatureCopy {
  /** The temporary grant command, taken apart: one note per argument. */
  grant: {
    caption: string
    notes: { capability: string; duration: string; reason: string }
    reason: string
  }
}

interface IosCopy extends FeatureCopy {
  facts: string[]
  /** Alternative text of the iOS captures. */
  phones: Record<PhoneName, string>
  /** Accessible name of the row of iOS captures, which scrolls sideways on a narrow screen. */
  phonesLabel: string
}

/** A footnote. One that ends in a link reads `text`, then the linked `link.label`, then `after`. */
export type NoteCopy = string | { after: string; link: LinkCopy; text: string }

interface StatCopy {
  l: string
  v: string
}

interface HowCopy {
  diagram: {
    agents: string
    agentBin: string
    wire: string
    wireNote: string
    server: string
    serverNote: string
    serverRows: string[]
    clients: PointCopy[]
  }
  h2: string
  /** Accessible name of a stat's mark, which links to its footnote: read as "<noteRef> <number>". */
  noteRef: string
  /**
   * Footnotes to the stats, one per stat in the same order: the list numbers them, and each stat's mark is its
   * position and links to its note. Both are tuples so a stat cannot lose its note.
   */
  notes: [NoteCopy, NoteCopy, NoteCopy, NoteCopy]
  notesTitle: string
  stats: [StatCopy, StatCopy, StatCopy, StatCopy]
  /** One step per command: the server install, then the agent enrollment. */
  steps: [{ d: string; t: string }, { d: string; t: string }]
}

export interface LandingCopy {
  dash: FeatureCopy
  faq: { h2: string; items: { q: string; a: string }[] }
  final: { cta2: string; h2: string; sub: string }
  footer: { tagline: string; links: LinkCopy[]; linksLabel: string; legal: string }
  hero: HeroCopy
  how: HowCopy
  ios: IosCopy
  ipq: FeatureCopy
  /** Search and social copy. */
  meta: { description: string; title: string }
  more: { groups: { items: PointCopy[]; t: string }[]; h2: string }
  nav: NavCopy
  network: FeatureCopy
  security: SecurityCopy
  /** Alternative text of the product captures. */
  shots: Record<ShotName, string>
}

const zh: LandingCopy = {
  meta: {
    title: 'ServerBee：自托管的 VPS 监控，细到每一条线路',
    description:
      '一个面板看清所有 VPS：31 个省份的三网延迟、流媒体和 AI 解锁、每台机器每天花掉多少钱。拖拽式仪表盘和原生 iOS App 都是自带的。'
  },
  shots: {
    fleet: 'ServerBee 服务器页面：十二台服务器的卡片，显示 CPU、内存、磁盘、流量、延迟和每月费用。',
    network: '一台香港服务器的网络质量页面：按电信、联通、移动分栏的延迟与丢包，以及 24 小时延迟曲线。',
    'ip-quality': 'IP 质量页面：每台服务器的出口 IP、风险评分，以及各项流媒体和 AI 服务的解锁矩阵。',
    capabilities: '能力开关页面：每台服务器已启用的能力，其中一台带有一条临时终端授权。',
    dashboard: '自定义仪表盘：统计数值、CPU 对比曲线、仪表、内存排行、可用时间线、流量柱状图和世界地图。'
  },
  nav: {
    links: [
      {
        label: '网络质量',
        href: '#network'
      },
      {
        label: 'IP 质量',
        href: '#ipq'
      },
      {
        label: '安全',
        href: '#security'
      },
      {
        label: '仪表盘',
        href: '#dash'
      },
      {
        label: 'iOS',
        href: '#ios'
      },
      {
        label: '文档',
        href: docsUrl('zh', 'quick-start')
      }
    ],
    cta: '部署',
    githubLabel: 'GitHub 仓库',
    skip: '跳到正文',
    label: '主要',
    langBtn: 'English',
    themeLabel: '切换浅色或深色'
  },
  hero: {
    release: 'v{version} 已发布',
    releaseLink: '更新日志',
    h1a: '自托管的 VPS 监控，',
    h1b: '细到每一条线路。',
    sub: '一个面板看清所有 VPS：31 个省份的三网延迟、流媒体和 AI 解锁、每台机器每天花掉多少钱。',
    cta2: '看在线状态页',
    installLabel: '安装方式',
    tabs: {
      docker: 'Docker',
      binary: '二进制',
      railway: 'Railway'
    },
    railwayText: '在 Railway 上一键部署，自带 HTTPS 和持久化存储。',
    railwayBtn: '打开 Railway 模板',
    installNote: '装完会打印面板地址和一次性管理员密码。',
    copy: '复制',
    copied: '已复制',
    copiedStatus: '命令已复制',
    copyFailed: '复制失败',
    copyFailedStatus: '复制失败，命令已选中，可以手动复制。',
    facts: ['服务端是单个可执行文件，内置网页界面和 SQLite', '终端、远程执行默认关闭', 'AGPL-3.0-or-later 开源']
  },
  network: {
    h2: '31 个省份的三网延迟，一张图看完。',
    lede: '内置 96 个探测目标：31 个省份的电信、联通、移动，加上 Cloudflare、Google DNS 和 AWS 东京。每台服务器挑最多 20 个，延迟和丢包实时画在图上；按运营商一拆，哪条线在晚高峰掉链子一眼就看出来。',
    points: [
      {
        t: '96 个预设目标',
        d: '31 省 × 三网，外加 3 个国际节点。直接编译在程序里，不用自己维护 IP 列表。'
      },
      {
        t: '按运营商对比',
        d: '电信、联通、移动分栏并排，平均延迟和丢包各算各的。'
      },
      {
        t: '从实时到 30 天',
        d: '30–600 秒探测一轮。原始数据保留 7 天，小时汇总保留 90 天，可导出 CSV。'
      },
      {
        t: '内置路由追踪',
        d: '集成 trippy，不装 mtr 也能跑 ICMP / UDP / TCP 路由追踪，逐跳显示丢包和延迟。'
      }
    ],
    link: {
      label: '网络质量文档',
      href: docsUrl('zh', 'monitoring', '网络质量监控')
    }
  },
  ipq: {
    h2: '这个 IP 能解锁什么，一张表说清。',
    lede: '每台服务器用自己的出口 IP 检测 9 项服务。默认每 12 小时跑一次，出口 IP 一变就重新检测，也可以随时手动触发。',
    points: [
      {
        t: '流媒体、AI、社交',
        d: 'Netflix 能区分全解锁和仅自制剧，ChatGPT 会标出识别到的地区。'
      },
      {
        t: 'IP 风险画像',
        d: '住宅、数据中心等类型，以及代理、VPN、Tor 出口、已知滥用等标记。配置 ipapi.is 密钥后显示 0–100 风险分。'
      },
      {
        t: '自定义检测',
        d: '按状态码、响应内容或跳转地址写匹配规则，不写代码也能加自己的服务。'
      },
      {
        t: '可以公开展示',
        d: '状态页可以展示解锁结果，访客看到的 IP 自动打码。'
      }
    ],
    link: {
      label: 'IP 质量文档',
      href: docsUrl('zh', 'ip-quality')
    }
  },
  security: {
    h2: '探针不该是后门。',
    lede: '终端、远程执行、文件管理、Docker 这四项高危能力默认关闭。能开什么由 Agent 所在的主机决定，服务端只能读取，不能修改。临时要用，就在主机上授权一段时间，到期自动收回。',
    points: [
      {
        t: '高危能力默认关闭',
        d: '默认只开启探测、安全事件、防火墙黑名单、IP 质量、Tailscale 和升级。'
      },
      {
        t: '只有主机能打开',
        d: '能力写在 Agent 的配置里。面板和 API 都没法远程打开终端。'
      },
      {
        t: '临时授权，到期收回',
        d: '默认最长 24 小时（主机可调）。到期时，正在进行的终端会话也会一起断开。'
      },
      {
        t: '每一步都有记录',
        d: '终端会话的开启与关闭、命令执行、文件读写、能力授权都写进审计日志，保留 180 天。'
      }
    ],
    grant: {
      caption: '在这台 VPS 上执行',
      reason: '排查磁盘',
      notes: {
        capability: '要临时打开的能力',
        duration: '30 分钟后自动收回',
        reason: '原因随授权一起留档'
      }
    },
    link: {
      label: '能力与授权文档',
      href: docsUrl('zh', 'capabilities')
    }
  },
  dash: {
    h2: '监控墙，按你的习惯摆。',
    lede: '18 种内置小组件，在 12 列网格上拖拽、缩放、锁定。运维总览、客户看板、机房大屏，想建几个就建几个。',
    points: [
      {
        t: '实时、图表、状态三类',
        d: '数值统计、仪表、多线对比、流量柱状图、世界地图、可用时间线、Markdown 值班手册……'
      },
      {
        t: '多个仪表盘',
        d: '在顶部随时切换，可以设默认。管理员编辑，成员只读。'
      },
      {
        t: 'Widget SDK',
        d: '用 @serverbee/widget-sdk 写自己的组件，以 .js、.zip 或 HTTPS 链接安装。'
      }
    ],
    link: {
      label: '仪表盘文档',
      href: docsUrl('zh', 'dashboards')
    }
  },
  ios: {
    h2: '整个机群，装进 iPhone。',
    facts: ['开源', 'App Store 即将上架', 'iOS {ios}+'],
    phonesLabel: 'App 界面',
    phones: {
      servers: 'iOS App 的服务器列表：按分组显示十二台服务器，每台都有实时的 CPU、内存和磁盘占用条。',
      detail: 'iOS 上的服务器概览：CPU、内存、磁盘和网络四块数据，下面是运行时信息。',
      insights: 'iOS 上的 Insights 页：机群总览、实时流量、每月费用，以及流量、安全、IP 质量和网络探测的入口。'
    },
    lede: '原生 SwiftUI 应用，只用 Apple 自家框架，没有第三方 SDK，也不做统计。服务器、告警、网络质量、IP 质量、Tailscale 和 Docker 都能在手机上看。',
    points: [
      {
        t: '扫码登录',
        d: '在网页端生成二维码，手机一扫就登录，不用输地址和密码。二维码 5 分钟内有效，只能用一次。'
      },
      {
        t: '告警推送',
        d: '告警在服务端加密，经 Relay 以密文转发到 APNs，到手机上才解密，点开直达对应服务器。自行构建的版本需要自备 Relay。'
      },
      {
        t: '随手处置',
        d: '管理员可以在手机上重启容器、封禁攻击来源 IP、安排维护窗口。'
      },
      {
        t: '中英双语',
        d: '界面完整本地化。隐私模式会隐藏 IP 后两段，截图分享更放心。'
      }
    ],
    link: {
      label: 'iOS 文档',
      href: docsUrl('zh', 'mobile')
    }
  },
  more: {
    h2: '其他功能，也都在里面。',
    groups: [
      {
        t: '监控与告警',
        items: [
          {
            t: '告警与通知',
            d: '26 种规则。资源指标要 10 分钟内 70% 的样本超标才触发，不会被瞬时尖峰误报；维护窗口内自动静默。支持 Webhook、Telegram、Bark、邮件和 APNs。'
          },
          {
            t: '公开状态页',
            d: '一个 /status 页面，展示在线状态、90 天可用率、网络和 IP 质量，IP 地址不会公开。默认关闭。'
          },
          {
            t: '服务监控',
            d: '由服务端检查 SSL 证书、DNS 记录、HTTP 关键词、TCP 端口和域名 WHOIS 到期。'
          },
          {
            t: '流量与成本',
            d: '按账单日计算流量周期并预测是否超额。折算每核、每 GB、每 TB 的成本，提醒离线还在计费、闲置烧钱的机器。'
          }
        ]
      },
      {
        t: '远程运维',
        items: [
          {
            t: '网页终端',
            d: '浏览器里的真实 PTY，每台最多 3 个会话，闲置 10 分钟自动断开。默认关闭。'
          },
          {
            t: '文件管理',
            d: '限定在指定根目录内浏览、上传、下载，用 Monaco 直接改配置。默认拒绝 *.key、.env 等敏感文件。'
          },
          {
            t: 'Docker',
            d: '容器列表、实时资源、日志流和事件，支持启动、停止、重启、删除。默认关闭。'
          },
          {
            t: '升级与运维',
            d: '在面板里逐台升级 Agent：校验 SHA-256，失败自动回滚。另有支持 cron 的定时命令（需开启远程执行）。'
          }
        ]
      },
      {
        t: '安全与访问',
        items: [
          {
            t: '安全事件与防火墙',
            d: '识别 SSH 暴力破解、新来源登录和端口扫描，可以把来源 IP 加进 nftables 黑名单。'
          },
          {
            t: '账号与权限',
            d: '管理员与只读成员，GitHub、Google、OIDC 与 Tailscale Serve 回环登录，TOTP 两步验证，API 密钥。'
          },
          {
            t: 'Tailscale 与 Headscale',
            d: 'Agent 可以走 Tailnet 接入，面板不必暴露在公网。还能看到每个对端是直连还是经 DERP 中转。'
          }
        ]
      }
    ]
  },
  how: {
    h2: '两条命令，跑起来。',
    diagram: {
      agents: '你的 VPS',
      agentBin: 'serverbee-agent',
      wire: 'WebSocket',
      wireNote: 'Agent 主动连接，VPS 无需开放入站端口',
      server: 'ServerBee 服务端',
      serverNote: '单个可执行文件',
      serverRows: ['REST API 与 WebSocket', '网页界面（内置）', 'SQLite（内置）'],
      clients: [
        {
          t: '浏览器',
          d: '网页面板、公开状态页'
        },
        {
          t: 'iOS App',
          d: '实时数据与告警推送'
        },
        {
          t: '通知渠道',
          d: 'Webhook、Telegram、Bark、邮件、APNs'
        }
      ]
    },
    steps: [
      {
        t: '部署服务端',
        d: '用 Docker 或二进制，一条命令装好。完成后会打印面板地址和一次性管理员密码。也可以用 Railway 一键部署。'
      },
      {
        t: '添加服务器',
        d: '在面板里点「添加服务器」，把生成的命令粘到 VPS 上执行。注册码只能用一次，10 分钟内有效。'
      }
    ],
    stats: [
      {
        v: '3 秒',
        l: '指标上报间隔'
      },
      {
        v: '13.7 MB',
        l: 'Agent 可执行文件'
      },
      {
        v: '≈ 27 MB',
        l: 'Agent 常驻内存'
      },
      {
        v: '3,800+',
        l: '自动化测试'
      }
    ],
    noteRef: '来源',
    notesTitle: '数据来源',
    notes: [
      '由服务端统一下发，每份上报到达后立即推送给浏览器。',
      'v1.0.0-beta.4 发布包 linux-amd64；linux-arm64 为 12.4 MB。',
      'v0.9.3 实测：4 核 KVM，运行 8 小时后的 cgroup 内存。1.0 版本待复测。',
      {
        text: 'Rust 与前端测试合计，见',
        link: {
          label: '测试文档',
          href: docsUrl('zh', 'testing')
        },
        after: '。'
      }
    ]
  },
  faq: {
    h2: '常见问题',
    items: [
      {
        q: '收费吗？',
        a: '不收费。ServerBee 以 AGPL-3.0-or-later 协议开源，部署在你自己的机器上，数据也留在你手里。'
      },
      {
        q: 'VPS 上需要开放端口吗？',
        a: '不需要。Agent 主动连接服务端的 WebSocket，被监控的机器不用开放入站端口。服务端建议放在 HTTPS 后面，安装脚本的 --domain 参数可以自动配置 Caddy 和证书。'
      },
      {
        q: '三网延迟测的是什么？',
        a: 'Agent 从你的 VPS 向各省电信、联通、移动的探测点发起 TCP 连接，记录往返延迟和丢包。探测点是第三方 CDN 节点，结果反映这台 VPS 到各运营商的线路质量，不等同于某位用户的真实体验。'
      },
      {
        q: '能公开给别人看吗？',
        a: '可以。开启公开状态页后，访客能看到在线状态、可用率、网络和 IP 质量，IP 地址会被隐藏。状态页默认关闭。'
      },
      {
        q: '支持哪些系统？',
        a: '服务端和 Agent 都提供 Linux amd64 / arm64 静态二进制和 Docker 镜像，安装脚本支持 systemd 和 OpenRC。macOS 与 Windows 也有构建，其中 Windows 为基础支持；安全事件和防火墙仅限 Linux。'
      },
      {
        q: 'iOS App 在哪下载？',
        a: '即将上架 App Store。在那之前，可以用 Xcode 从仓库的 apps/ios 目录构建，安装到 iOS {ios} 或更高版本的 iPhone。'
      }
    ]
  },
  final: {
    h2: '把第一台 VPS 放上雷达。',
    sub: '一条命令装好服务端，每台 VPS 再一条命令接入。',
    cta2: '在 GitHub 上查看'
  },
  footer: {
    tagline: '自托管的 VPS 监控。',
    linksLabel: '页脚',
    links: [
      {
        label: '文档',
        href: docsUrl('zh', 'quick-start')
      },
      {
        label: 'GitHub',
        href: links.github
      },
      {
        label: '更新日志',
        href: links.releases
      },
      {
        label: '状态页演示',
        href: links.status
      }
    ],
    legal: '以 AGPL-3.0-or-later 协议开源。'
  }
}

const en: LandingCopy = {
  meta: {
    title: 'ServerBee: self-hosted VPS monitoring, down to every route',
    description:
      'Self-hosted VPS monitoring with live metrics, network quality, IP reputation, alerts and costs in one panel, plus drag-and-drop dashboards and a native iOS app.'
  },
  shots: {
    fleet:
      'The ServerBee servers page: twelve server cards showing CPU, memory, disk, traffic, latency and monthly cost.',
    network:
      'Network quality for a Hong Kong server: latency and loss split by China Telecom, Unicom and Mobile, above a 24-hour latency chart.',
    'ip-quality':
      'The IP quality page: each server’s egress IP and risk score, and a matrix of which streaming and AI services it unlocks.',
    capabilities:
      'The capabilities page: which capabilities each server has enabled, one of them holding a temporary terminal grant.',
    dashboard:
      'A custom dashboard with stat tiles, a CPU comparison chart, gauges, a memory ranking, uptime timelines, traffic bars and a world map.'
  },
  nav: {
    links: [
      {
        label: 'Dashboards',
        href: '#dash'
      },
      {
        label: 'Network',
        href: '#network'
      },
      {
        label: 'IP quality',
        href: '#ipq'
      },
      {
        label: 'Security',
        href: '#security'
      },
      {
        label: 'iOS',
        href: '#ios'
      },
      {
        label: 'Docs',
        href: docsUrl('en', 'quick-start')
      }
    ],
    cta: 'Install',
    githubLabel: 'GitHub repository',
    skip: 'Skip to content',
    label: 'Main',
    langBtn: '中文',
    themeLabel: 'Toggle light or dark'
  },
  hero: {
    release: 'v{version} is out',
    releaseLink: 'Changelog',
    h1a: 'Self-hosted VPS monitoring,',
    h1b: 'down to every route.',
    sub: 'Live metrics, network quality, IP reputation, alerts and costs for every VPS you run, in one self-hosted panel.',
    cta2: 'See a live status page',
    installLabel: 'Install method',
    tabs: {
      docker: 'Docker',
      binary: 'Binary',
      railway: 'Railway'
    },
    railwayText: 'One-click deploy on Railway, with HTTPS and a persistent volume.',
    railwayBtn: 'Open the Railway template',
    installNote: 'The installer prints your panel URL and a one-time admin password.',
    copy: 'Copy',
    copied: 'Copied',
    copiedStatus: 'Command copied',
    copyFailed: 'Copy failed',
    copyFailedStatus: 'Copy failed. The command is selected, so you can copy it yourself.',
    facts: [
      'One server binary with the web UI and SQLite built in',
      'Terminal and remote exec off by default',
      'Open source, AGPL-3.0-or-later'
    ]
  },
  network: {
    h2: 'Latency and loss, carrier by carrier.',
    lede: '96 probe targets ship with the server: China Telecom, Unicom and Mobile in all 31 provinces, plus Cloudflare, Google DNS and AWS Tokyo. Assign up to 20 per server and watch latency and loss live, or split by carrier to see which line drops first at peak hours.',
    points: [
      {
        t: '96 preset targets',
        d: '31 provinces × three carriers, plus 3 international anchors. Compiled in, so there is no IP list to maintain.'
      },
      {
        t: 'Split by carrier',
        d: 'Telecom, Unicom and Mobile side by side, each with its own average latency and loss.'
      },
      {
        t: 'Real time to 30 days',
        d: 'Probe every 30–600 s. Raw data is kept 7 days, hourly rollups 90 days, with CSV export.'
      },
      {
        t: 'Built-in traceroute',
        d: 'ICMP, UDP or TCP traceroute from the agent via embedded trippy, no mtr install needed.'
      }
    ],
    link: {
      label: 'Network quality docs',
      href: docsUrl('en', 'monitoring', 'network-quality-monitoring')
    }
  },
  ipq: {
    h2: 'What each IP can unlock, in one table.',
    lede: 'Every server checks 9 services from its own egress IP. Checks run every 12 hours, again as soon as the IP changes, and whenever you ask.',
    points: [
      {
        t: 'Streaming, AI, social',
        d: 'Netflix separates full access from originals-only, and ChatGPT reports the detected region.'
      },
      {
        t: 'IP risk profile',
        d: 'IP type plus proxy, VPN, Tor, abuser and hosting flags. Add an ipapi.is key for a 0–100 risk score.'
      },
      {
        t: 'Custom checks',
        d: 'Match on status code, response body or redirect URL to add your own services, no code needed.'
      },
      {
        t: 'Shareable',
        d: 'Show unlock results on your status page, with the IP masked for visitors.'
      }
    ],
    link: {
      label: 'IP quality docs',
      href: docsUrl('en', 'ip-quality')
    }
  },
  security: {
    h2: 'A monitoring agent shouldn’t be a backdoor.',
    lede: 'Terminal, remote exec, file manager and Docker ship switched off. The host running the agent decides what it may do; the server can read that setting but never change it. Need a shell for half an hour? Grant it on the host and it expires on its own.',
    points: [
      {
        t: 'High-risk off by default',
        d: 'The default set covers probes, security events, the firewall blocklist, IP quality, Tailscale and upgrades.'
      },
      {
        t: 'Only the host can turn them on',
        d: 'Capabilities live in the agent’s config. Neither the panel nor the API can open a terminal remotely.'
      },
      {
        t: 'Grants that expire',
        d: '24 hours max by default (the host can change it). When a grant lapses, live terminal sessions close with it.'
      },
      {
        t: 'Everything leaves a trail',
        d: 'Terminal sessions, exec runs, file operations and capability grants are audit-logged for 180 days.'
      }
    ],
    grant: {
      caption: 'Run on the VPS itself',
      reason: 'disk check',
      notes: {
        capability: 'The capability to switch on',
        duration: 'Revoked on its own after 30 minutes',
        reason: 'Recorded with the grant'
      }
    },
    link: {
      label: 'Capabilities docs',
      href: docsUrl('en', 'capabilities')
    }
  },
  dash: {
    h2: 'Build the wall you actually want to look at.',
    lede: '18 built-in widgets on a 12-column grid: drag, resize, lock. Build an ops overview, a customer board or a wallboard, as many as you need.',
    points: [
      {
        t: 'Real-time, charts, status',
        d: 'Stat tiles, gauges, multi-line comparisons, traffic bars, a world map, uptime timelines, Markdown runbooks…'
      },
      {
        t: 'Many dashboards',
        d: 'Switch from the header and set a default. Admins edit; members get read-only views.'
      },
      {
        t: 'Widget SDK',
        d: 'Write your own widgets with @serverbee/widget-sdk and install them as a .js file, a .zip pack or an HTTPS URL.'
      }
    ],
    link: {
      label: 'Dashboard docs',
      href: docsUrl('en', 'dashboards')
    }
  },
  ios: {
    h2: 'Your whole fleet, in your pocket.',
    facts: ['Open source', 'App Store soon', 'iOS {ios}+'],
    phonesLabel: 'App screens',
    phones: {
      servers: 'The iOS app’s server list: twelve servers in groups, each with live CPU, memory and disk bars.',
      detail: 'A server’s overview on iOS: CPU, memory, disk and network tiles above its runtime details.',
      insights:
        'The Insights tab on iOS: fleet totals, live traffic, monthly cost, and shortcuts to traffic, security, IP quality and network probes.'
    },
    lede: 'A native SwiftUI app built only on Apple frameworks, with no third-party SDKs and no analytics. Servers, alerts, network quality, IP quality, Tailscale and Docker, all on your phone.',
    points: [
      {
        t: 'Scan to sign in',
        d: 'Show a QR code in the web panel and scan it. No URL, no password. Codes expire after 5 minutes and work once.'
      },
      {
        t: 'Push alerts',
        d: 'Alerts are encrypted on your server, relayed to APNs as ciphertext and decrypted on the phone. Tapping one opens the affected server. Source builds need their own relay.'
      },
      {
        t: 'Act from anywhere',
        d: 'Admins can restart containers, block an attacking IP or schedule maintenance from the phone.'
      },
      {
        t: 'Bilingual and private',
        d: 'Fully localized in English and Chinese. Privacy mode masks IPs before you share a screenshot.'
      }
    ],
    link: {
      label: 'iOS docs',
      href: docsUrl('en', 'mobile')
    }
  },
  more: {
    h2: 'And the rest of the toolbox.',
    groups: [
      {
        t: 'Monitoring and alerts',
        items: [
          {
            t: 'Alerts & notifications',
            d: '26 rule types. Metric thresholds fire only when 70% of samples in 10 minutes breach, so a single spike won’t page you; maintenance windows mute alerts. Webhook, Telegram, Bark, email and APNs.'
          },
          {
            t: 'Public status page',
            d: 'One /status page with live health, 90-day uptime, network and IP quality. Addresses stay hidden. Off until you enable it.'
          },
          {
            t: 'Service monitors',
            d: 'SSL expiry, DNS records, HTTP keywords, TCP ports and WHOIS expiry, checked from the server.'
          },
          {
            t: 'Traffic & cost',
            d: 'Traffic cycles that follow your billing day, with overage projection. Cost per core, GB and TB, plus flags for idle or offline boxes you still pay for.'
          }
        ]
      },
      {
        t: 'Remote operations',
        items: [
          {
            t: 'Web terminal',
            d: 'A real PTY in the browser, up to 3 sessions per server, closed after 10 idle minutes. Off by default.'
          },
          {
            t: 'File manager',
            d: 'Browse, upload and download inside allowed root paths, and edit configs in Monaco. Keys and .env files are denied by default.'
          },
          {
            t: 'Docker',
            d: 'Containers with live stats, log streaming and events; start, stop, restart, remove. Off by default.'
          },
          {
            t: 'Upgrades & ops',
            d: 'Upgrade agents from the panel one at a time: SHA-256 verified, rolled back on failure. Scheduled commands with cron (needs remote exec).'
          }
        ]
      },
      {
        t: 'Security and access',
        items: [
          {
            t: 'Security events & firewall',
            d: 'Detects SSH brute force, new-source logins and port scans, and can drop the source IP into an nftables blocklist.'
          },
          {
            t: 'Accounts & access',
            d: 'Admins and read-only members, GitHub, Google, OIDC and Tailscale Serve loopback sign-in, TOTP 2FA and API keys.'
          },
          {
            t: 'Tailscale & Headscale',
            d: 'Enroll agents over your tailnet, so the panel never has to face the public internet. See whether each peer connects directly or through a DERP relay.'
          }
        ]
      }
    ]
  },
  how: {
    h2: 'Two commands and you’re live.',
    diagram: {
      agents: 'Your servers',
      agentBin: 'serverbee-agent',
      wire: 'WebSocket',
      wireNote: 'Agents dial out, so no inbound ports on your VPS',
      server: 'ServerBee server',
      serverNote: 'One executable',
      serverRows: ['REST API and WebSocket', 'Web UI (built in)', 'SQLite (built in)'],
      clients: [
        {
          t: 'Browser',
          d: 'Web panel and public status page'
        },
        {
          t: 'iOS app',
          d: 'Live data and push alerts'
        },
        {
          t: 'Notifications',
          d: 'Webhook, Telegram, Bark, email, APNs'
        }
      ]
    },
    steps: [
      {
        t: 'Install the server',
        d: 'Docker or a binary, one command. It prints the panel URL and a one-time admin password. Or deploy on Railway.'
      },
      {
        t: 'Add a server',
        d: 'Click Add server in the panel and paste the generated command on your VPS. The enrollment code works once, within 10 minutes.'
      }
    ],
    stats: [
      {
        v: '3 s',
        l: 'metrics interval'
      },
      {
        v: '13.7 MB',
        l: 'agent binary'
      },
      {
        v: '≈ 27 MB',
        l: 'agent memory'
      },
      {
        v: '3,800+',
        l: 'automated tests'
      }
    ],
    noteRef: 'Source',
    notesTitle: 'Sources',
    notes: [
      'Set by the server; each report is pushed to browsers as it arrives.',
      'v1.0.0-beta.4 linux-amd64 release asset; linux-arm64 is 12.4 MB.',
      'Measured on v0.9.3: 4-core KVM, cgroup memory after 8 hours. Not yet re-measured on 1.0.',
      {
        text: 'Rust and frontend tests combined; see the ',
        link: {
          label: 'testing docs',
          href: docsUrl('en', 'testing')
        },
        after: '.'
      }
    ]
  },
  faq: {
    h2: 'Questions',
    items: [
      {
        q: 'Does it cost anything?',
        a: 'No. ServerBee is open source under AGPL-3.0-or-later. You run it on your own machine, and the data stays with you.'
      },
      {
        q: 'Do my servers need open ports?',
        a: 'No. Agents dial out to the server over WebSocket, so monitored hosts need no inbound ports. Put the server behind HTTPS; the installer’s --domain flag sets up Caddy and a certificate.'
      },
      {
        q: 'What does the carrier latency measure?',
        a: 'The agent opens TCP connections from your VPS to probe endpoints for each province and carrier, and records round-trip latency and loss. The endpoints are third-party CDN nodes, so read the numbers as path quality, not any single user’s experience.'
      },
      {
        q: 'Can I share it publicly?',
        a: 'Yes. Turn on the public status page to show health, uptime, network and IP quality without exposing addresses. It’s off by default.'
      },
      {
        q: 'Which systems are supported?',
        a: 'Static Linux binaries for amd64 and arm64, plus Docker images, for both the server and the agent. The installer handles systemd and OpenRC. macOS and Windows builds exist (Windows support is basic); security events and the firewall are Linux-only.'
      },
      {
        q: 'Where do I get the iOS app?',
        a: 'It’s coming to the App Store. Until then, build it from apps/ios in the repository with Xcode and install it on an iPhone running iOS {ios} or later.'
      }
    ]
  },
  final: {
    h2: 'Put your first VPS on the radar.',
    sub: 'One command for the server, one more for each VPS.',
    cta2: 'View on GitHub'
  },
  footer: {
    tagline: 'Self-hosted VPS monitoring.',
    linksLabel: 'Footer',
    links: [
      {
        label: 'Docs',
        href: docsUrl('en', 'quick-start')
      },
      {
        label: 'GitHub',
        href: links.github
      },
      {
        label: 'Changelog',
        href: links.releases
      },
      {
        label: 'Status page demo',
        href: links.status
      }
    ],
    legal: 'Open source under AGPL-3.0-or-later.'
  }
}

export const landingCopy: Record<Lang, LandingCopy> = { en, zh }
