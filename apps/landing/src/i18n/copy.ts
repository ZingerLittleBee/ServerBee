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
  /** The headline's second line. It ends in a full stop: the hero's routes are drawn leaving from it. */
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

/**
 * Text that only appears inside the animated scenes. It replays what the page already says in prose, so all of it
 * is hidden from assistive technology.
 */
interface ScenesCopy {
  /** The alert that leaves the architecture diagram through a notification channel. */
  alert: string
  /** The carriers whose latency rides the hero's routes, in the order of the network capture's columns. */
  carriers: [string, string, string]
  /** The temporary grant as it takes effect and lapses. `{left}` is the time remaining, as mm:ss. */
  grant: {
    /** The badge in the capability table. It repeats the product's own wording, as the capture shows it. */
    badge: string
    granted: string
    revoked: string
  }
  /** The push notification that lands on the first phone, for the server the second phone shows. */
  push: { body: string; time: string }
  /** Stops of the page rail that the header navigation has no name for. */
  rail: { faq: string; how: string }
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
  scenes: ScenesCopy
  security: SecurityCopy
  /** Alternative text of the product captures. */
  shots: Record<ShotName, string>
}

const zh: LandingCopy = {
  meta: {
    title: 'ServerBee：自托管的 VPS 监控，细到每一条线路',
    description:
      '自托管 VPS 监控面板：实时指标、31 省三网延迟、流媒体与 AI 解锁、每台机器的账单成本，附带可拖拽仪表盘和原生 iOS App。'
  },
  shots: {
    fleet: 'ServerBee 服务器页面：十二台服务器的卡片，显示 CPU、内存、磁盘、流量、延迟和每月费用。',
    network: '一台香港服务器的网络质量页面：按电信、联通、移动分栏的延迟与丢包，以及 24 小时延迟曲线。',
    'ip-quality': 'IP 质量页面：每台服务器的出口 IP、风险评分，以及各项流媒体和 AI 服务的解锁矩阵。',
    capabilities: '能力开关页面：每台服务器已启用的能力，其中一台带有一条临时终端授权。',
    dashboard: '自定义仪表盘：统计数值、CPU 对比曲线、仪表、内存排行、可用时间线、流量柱状图和世界地图。'
  },
  scenes: {
    carriers: ['电信', '联通', '移动'],
    grant: {
      granted: 'terminal 已授权 · 剩余 {left}',
      revoked: 'terminal 已到期，自动收回',
      badge: '临时 · {left}'
    },
    push: {
      time: '现在',
      body: 'HK · CN2 GIA 的 CPU 使用率 92%，已持续 10 分钟'
    },
    alert: '告警 → Telegram · CPU 92%',
    rail: {
      how: '部署',
      faq: '常见问题'
    }
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
    sub: '实时资源、31 省三网延迟、IP 解锁、每台机器的成本，都在自己搭的这一个面板里。',
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
    facts: ['单文件服务端，内置 Web 面板与 SQLite', '终端与远程执行默认关闭', 'AGPL-3.0-or-later 开源']
  },
  network: {
    h2: '三网 31 省延迟，晚高峰见真章。',
    lede: '给每台 VPS 挑最多 20 个探测点，实时追踪延迟和丢包。按电信、联通、移动拆开对比，晚高峰哪条线先抖、哪条线稳，看图就知道。',
    points: [
      {
        t: '96 个内置目标',
        d: '覆盖 31 省三网和 Cloudflare、Google DNS、AWS 东京，编译在服务端里，不用自己找测速 IP。'
      },
      {
        t: '分运营商统计',
        d: '电信、联通、移动并排展示，平均延迟与丢包率各自核算，互不掺杂。'
      },
      {
        t: '从实时到 30 天',
        d: '30 到 600 秒探测一轮，原始数据存 7 天，小时汇总存 90 天，随时导出 CSV。'
      },
      {
        t: '内置路由追踪',
        d: '内嵌 trippy，不必在机器上装 mtr，直接跑 ICMP、UDP 或 TCP 路由追踪，逐跳看丢包和抖动。'
      }
    ],
    link: {
      label: '网络质量文档',
      href: docsUrl('zh', 'monitoring', '网络质量监控')
    }
  },
  ipq: {
    h2: '这个 IP 干不干净，能解锁什么？',
    lede: '刚开好的机器，或者换过一次出口 IP，不用再手动跑脚本。每台 Agent 用出口 IP 定期检测 9 项服务解锁，服务端补上风控标记，IP 一变就自动复测。',
    points: [
      {
        t: '流媒体、AI、社交',
        d: '内置 9 项常用服务，默认每 12 小时测一次，也可随时手动跑。Netflix 区分全解锁和仅自制剧，ChatGPT 标出识别地区。'
      },
      {
        t: 'IP 风控画像',
        d: '标注住宅、机房等类型，以及代理、VPN、Tor 出口和滥用记录。填入 ipapi.is 密钥可算 0 到 100 风险分。'
      },
      {
        t: '自定义检测规则',
        d: '按状态码、响应体或跳转地址配匹配条件，不用写代码就能把自己的业务加进检测表。'
      },
      {
        t: '状态页脱敏展示',
        d: '公开状态页可选展示解锁矩阵，访客看到的出口 IP 自动打码。'
      }
    ],
    link: {
      label: 'IP 质量文档',
      href: docsUrl('zh', 'ip-quality')
    }
  },
  security: {
    h2: '探针不该是后门。',
    lede: '就算面板账号失守，也不该顺手交出所有机器的 Shell。每台主机自己决定对外开放哪些能力，服务端只读不改；临时要排查问题，在主机上给一段限时授权，到点自动关掉。',
    points: [
      {
        t: '四项高危能力默认关闭',
        d: '终端、远程执行、文件管理和 Docker 默认不开启，出厂只留探测、安全事件、防火墙黑名单、IP 质量、Tailscale 和升级。'
      },
      {
        t: '权限归宿主机管',
        d: '能力开关写在 Agent 本地配置里，面板和 API 没有任何远程开启终端的入口。'
      },
      {
        t: '限时授权，到点断连',
        d: '临时授权默认最长 24 小时（主机可调）。时间一到，正在进行的终端会话也会一并断开。'
      },
      {
        t: '操作留痕 180 天',
        d: '终端开闭、命令执行、文件读写和能力授权全进审计日志，默认保留 180 天。'
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
    lede: '18 种内置组件，在 12 列网格里拖拽、缩放和锁定。日常总览、三网线路页、按地区或项目拆开的看板，随你建几张。',
    points: [
      {
        t: '实时、图表、状态三类',
        d: '汇总数值、仪表盘、多机对比曲线、流量柱状图、世界地图、可用性时间线，还能放 Markdown 值班笔记。'
      },
      {
        t: '多套布局按需切',
        d: '顶部栏随时切换并指定默认页。管理员编排布局，普通成员只读查看。'
      },
      {
        t: 'Widget SDK 扩展',
        d: '用 @serverbee/widget-sdk 写自己的 React 组件，支持 .js 单文件、.zip 合集包或 HTTPS 链接安装。'
      }
    ],
    link: {
      label: '仪表盘文档',
      href: docsUrl('zh', 'dashboards')
    }
  },
  ios: {
    h2: '不在电脑前，iPhone 接着管。',
    facts: ['开源', 'App Store 即将上架', 'iOS {ios}+'],
    phonesLabel: 'App 界面',
    phones: {
      servers: 'iOS App 的服务器列表：按分组显示十二台服务器，每台都有实时的 CPU、内存和磁盘占用条。',
      detail: 'iOS 上的服务器概览：CPU、内存、磁盘和网络四块数据，下面是运行时信息。',
      insights: 'iOS 上的 Insights 页：机群总览、实时流量、每月费用，以及流量、安全、IP 质量和网络探测的入口。'
    },
    lede: 'SwiftUI 原生编写，只用 Apple 系统框架，零第三方 SDK，也不埋点。外出时收到告警，点开就能看指标、线路、IP 状态、Tailscale 和容器，顺手处理掉。',
    points: [
      {
        t: '扫码配对',
        d: '在网页面板生成二维码，iPhone 一扫即登，省去手敲地址和密码。二维码 5 分钟有效，单次使用。'
      },
      {
        t: '加密告警推送',
        d: '告警在你的服务端加密，经 Relay 以密文送往 APNs，到手机才解密，点开直达出事那台机器。自行编译需自备 Relay。'
      },
      {
        t: '不止能看，还能管',
        d: '管理员可在手机上重启容器、把攻击源 IP 拉进黑名单，或者顺手排一个维护窗口。'
      },
      {
        t: '双语与隐私打码',
        d: '完整支持中英文界面。打开隐私模式自动遮住 IP 后两段，发群里截图不用手动糊码。'
      }
    ],
    link: {
      label: 'iOS 文档',
      href: docsUrl('zh', 'mobile')
    }
  },
  more: {
    h2: '监控之外，日常运维也都备齐。',
    groups: [
      {
        t: '监控与告警',
        items: [
          {
            t: '告警与通知',
            d: '内置 26 种规则。资源告警要求 10 分钟内 70% 样本超标才触发，过滤掉瞬时毛刺；维护窗口内自动静默。支持 Webhook、Telegram、Bark、邮件和 APNs。'
          },
          {
            t: '公开状态页',
            d: '自带 /status 状态页，按需展示在线状态、90 天可用率、网络和 IP 质量，主机 IP 不会公开。默认关闭。'
          },
          {
            t: '服务监控',
            d: '由服务端定时检查 SSL 证书、DNS 记录、HTTP 关键词、TCP 端口和域名 WHOIS 到期，不装 Agent 的站点也能盯。'
          },
          {
            t: '流量与成本',
            d: '按你的账单日算流量周期并预测周期末会不会超额；折算每核、每 GB 和每 TB 的月均成本，直接标出离线还在扣费或闲置空转的机器。'
          }
        ]
      },
      {
        t: '远程运维',
        items: [
          {
            t: '网页终端',
            d: '浏览器里的真实 PTY，支持 vim 和 htop。每台机器最多 3 个并发会话，闲置 10 分钟自动断开，默认关闭。'
          },
          {
            t: '文件管理',
            d: '锁定在白名单根目录内浏览、传文件，用 Monaco 在线改配置。默认拦截 *.key、.env 等敏感文件。'
          },
          {
            t: 'Docker',
            d: '查看容器列表、实时资源、日志流与生命周期事件，支持启停、重启和删除。默认关闭。'
          },
          {
            t: '升级与运维',
            d: '在面板里逐台升级 Agent，校验 SHA-256 并在失败时自动回滚。也支持 cron 定时任务（需主机开启远程执行）。'
          }
        ]
      },
      {
        t: '安全与访问',
        items: [
          {
            t: '安全事件与防火墙',
            d: '识别 SSH 爆破和新来源登录，开启后还能识别端口扫描，支持把攻击源 IP 写入主机 nftables 黑名单。'
          },
          {
            t: '账号与权限',
            d: '区分管理员和只读成员，支持 GitHub、Google、OIDC、Tailscale Serve 回环认证、TOTP 两步验证和 API 密钥。'
          },
          {
            t: 'Tailscale 与 Headscale',
            d: 'Agent 可直接走 Tailnet 接入，面板无需暴露公网；并能一眼看出每个对端是 Direct 直连还是走了 DERP 中继。'
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
      wireNote: 'Agent 主动外连，VPS 不用开入站端口',
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
        d: 'Docker 或原生二进制都只要一条命令，内置网页端和 SQLite，省去外置数据库。也可以在 Railway 一键跑起来。'
      },
      {
        t: '接入服务器',
        d: '在面板里点「添加服务器」，把生成的命令贴到目标 VPS 执行。注册码单次有效，10 分钟过期，之后自动续连。'
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
        a: '免费且开源（AGPL-3.0-or-later）。服务跑在你自己的机器上，监控数据存在本地 SQLite 里。'
      },
      {
        q: '被监控的 VPS 要开端口吗？',
        a: '不用。Agent 主动外连服务端的 WebSocket，被监控机器不需要放行任何入站端口。服务端建议挂在 HTTPS 后面，安装脚本传 --domain 就能自动配好 Caddy 和证书。'
      },
      {
        q: '三网延迟到底测的是什么？',
        a: 'Agent 从你的 VPS 向各省电信、联通、移动的第三方 CDN 探测点发起 TCP 连接，记录往返延迟和丢包率。数据反映的是这台机器到三大运营商的线路质量，不代表某一条宽带的实际体感。'
      },
      {
        q: '能公开给别人看吗？',
        a: '可以。状态页默认关闭，手动开启后访客能看到在线状态、90 天可用率、网络和 IP 质量，机器真实 IP 会自动隐藏。'
      },
      {
        q: '支持哪些系统？',
        a: '服务端和 Agent 均提供 Linux amd64 / arm64 静态二进制和 Docker 镜像，安装脚本兼容 systemd 与 OpenRC。也有 macOS 和 Windows 构建（Windows 为基础支持），其中安全事件与防火墙仅限 Linux。'
      },
      {
        q: 'iOS App 在哪下载？',
        a: '即将上架 App Store。上架前可用 XcodeGen 和 Xcode 从仓库的 apps/ios 目录自行构建，支持 iOS {ios} 及以上版本的 iPhone。签名需使用你自己的 Apple 开发者团队。'
      }
    ]
  },
  final: {
    h2: '先把第一台 VPS 接进来。',
    sub: '一条命令装好服务端，在面板里复制注册命令，跑完就能看到实时曲线。',
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
      'Self-hosted VPS monitoring with one Rust server binary: live metrics, carrier probes, IP unlocks, host-owned security, custom dashboards and a native iOS app.'
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
  scenes: {
    carriers: ['Telecom', 'Unicom', 'Mobile'],
    grant: {
      granted: 'terminal granted · {left} left',
      revoked: 'terminal expired, revoked',
      badge: 'Temporary · {left}'
    },
    push: {
      time: 'now',
      body: 'HK · CN2 GIA: CPU at 92% for 10 minutes'
    },
    alert: 'Alert → Telegram · CPU 92%',
    rail: {
      how: 'Install',
      faq: 'FAQ'
    }
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
    sub: 'Track your fleet from one place: live load, carrier routes, IP unlock and risk, alerts, and what each server actually costs.',
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
    lede: 'Assign up to 20 probe targets per server and track latency and packet loss continuously. Split the view by carrier to catch evening congestion as soon as it starts.',
    points: [
      {
        t: '96 built-in targets',
        d: 'China Telecom, Unicom and Mobile across 31 provinces, plus Cloudflare, Google DNS and AWS Tokyo. Compiled in, with no IP list to maintain.'
      },
      {
        t: 'Carrier breakdown',
        d: 'Telecom, Unicom and Mobile side by side, each with its own average latency and packet loss.'
      },
      {
        t: 'Live stream to 30-day history',
        d: 'Probe every 30 to 600 s. Raw samples stay for 7 days, hourly rollups for 90 days, with CSV export.'
      },
      {
        t: 'Built-in traceroute',
        d: 'Run ICMP, UDP or TCP traces straight from the agent via embedded trippy, without installing mtr.'
      }
    ],
    link: {
      label: 'Network quality docs',
      href: docsUrl('en', 'monitoring', 'network-quality-monitoring')
    }
  },
  ipq: {
    h2: 'What each IP unlocks, and how clean it is.',
    lede: 'Stop SSHing in to run unlock scripts on every new box. Each agent tests 9 services from its own egress IP every 12 hours, and re-runs the moment the IP changes.',
    points: [
      {
        t: 'Streaming, AI and social',
        d: 'Separates full Netflix access from originals-only, reports the region ChatGPT sees, and lets you re-run on demand.'
      },
      {
        t: 'Reputation and risk flags',
        d: 'Surfaces IP type alongside proxy, VPN, Tor, abuser and hosting flags. Add an ipapi.is key for a 0 to 100 risk score.'
      },
      {
        t: 'Custom service checks',
        d: 'Add your own endpoints by matching on status code, response body or redirect URL, with no code to deploy.'
      },
      {
        t: 'Safe to publish',
        d: 'Expose the unlock matrix on your public status page while egress IPs stay masked for visitors.'
      }
    ],
    link: {
      label: 'IP quality docs',
      href: docsUrl('en', 'ip-quality')
    }
  },
  security: {
    h2: 'A monitoring agent shouldn’t be a backdoor.',
    lede: 'A compromised panel should never hand over root across your fleet. Each host decides what its agent is allowed to do; the server can read that policy, never edit it. Need a shell for 30 minutes? Grant it locally and let it expire.',
    points: [
      {
        t: 'High-risk off by default',
        d: 'Terminal, remote exec, file manager and Docker ship disabled. Default grants cover probes, security events, firewall blocklists, IP quality, Tailscale and upgrades.'
      },
      {
        t: 'Host-owned boundaries',
        d: 'Capabilities live in the agent’s own config. Neither the web panel nor the API can flip on a shell remotely.'
      },
      {
        t: 'Grants that cut off on time',
        d: 'Temporary grants cap at 24 hours by default (configurable on the host). When the timer runs out, active terminal sessions drop with it.'
      },
      {
        t: '180-day audit trail',
        d: 'Terminal sessions, exec runs, file access and capability grants all land in the audit log, kept for 180 days.'
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
    h2: 'Lay out the screen you actually watch.',
    lede: 'Drag, resize and lock 18 built-in widgets on a 12-column grid, and save as many views as you need: a daily overview, a network board, or separate pages by region and project.',
    points: [
      {
        t: 'Live stats, charts and status',
        d: 'Metric tiles, gauges, multi-server lines, traffic bars, a world map, uptime timelines and Markdown runbooks.'
      },
      {
        t: 'Multiple views, clean roles',
        d: 'Switch boards from the header and pick a default. Admins edit layouts; members stay read-only.'
      },
      {
        t: 'Extend with Widget SDK',
        d: 'Build custom React widgets with @serverbee/widget-sdk and install them from a .js file, a .zip bundle or an HTTPS URL.'
      }
    ],
    link: {
      label: 'Dashboard docs',
      href: docsUrl('en', 'dashboards')
    }
  },
  ios: {
    h2: 'A native iOS app for when an alert hits.',
    facts: ['Open source', 'App Store soon', 'iOS {ios}+'],
    phonesLabel: 'App screens',
    phones: {
      servers: 'The iOS app’s server list: twelve servers in groups, each with live CPU, memory and disk bars.',
      detail: 'A server’s overview on iOS: CPU, memory, disk and network tiles above its runtime details.',
      insights:
        'The Insights tab on iOS: fleet totals, live traffic, monthly cost, and shortcuts to traffic, security, IP quality and network probes.'
    },
    lede: 'Written in SwiftUI on Apple frameworks alone, with no third-party SDKs and no analytics. Check live load, routes, IP status, Tailscale and Docker without opening a laptop.',
    points: [
      {
        t: 'Scan to pair',
        d: 'Pop up a QR code in the web panel and scan it on your phone. No URLs or passwords to type; codes work once and expire in 5 minutes.'
      },
      {
        t: 'Encrypted push alerts',
        d: 'Alerts are encrypted on your server, relayed to APNs as ciphertext and decrypted on device. Tapping one opens the affected server. Source builds require your own relay.'
      },
      {
        t: 'Handle incidents on the go',
        d: 'Admins can restart containers, block an attacking IP or schedule a maintenance window straight from the app.'
      },
      {
        t: 'Bilingual, with privacy mode',
        d: 'Full English and Chinese support. Privacy mode masks IP addresses so you can share screenshots without redacting by hand.'
      }
    ],
    link: {
      label: 'iOS docs',
      href: docsUrl('en', 'mobile')
    }
  },
  more: {
    h2: 'The rest of your daily ops kit.',
    groups: [
      {
        t: 'Monitoring and alerts',
        items: [
          {
            t: 'Alerts & notifications',
            d: '26 rule types. Metric alerts only fire when 70% of samples over 10 minutes breach, so a brief spike won’t wake you up; maintenance windows mute noise. Webhook, Telegram, Bark, email and APNs.'
          },
          {
            t: 'Public status page',
            d: 'A built-in /status page with live health, 90-day uptime, network and IP quality. Server IPs are not shown, and the page stays off until you enable it.'
          },
          {
            t: 'Service monitors',
            d: 'Server-side checks for SSL expiry, DNS records, HTTP keywords, TCP ports and WHOIS expiry, even for targets without an agent.'
          },
          {
            t: 'Traffic & cost',
            d: 'Traffic cycles aligned to your billing day, with end-of-cycle overage forecasts. Breaks down cost per core, GB and TB, and flags idle or offline boxes you’re still paying for.'
          }
        ]
      },
      {
        t: 'Remote operations',
        items: [
          {
            t: 'Web terminal',
            d: 'A full PTY in the browser, ready for vim or htop. Up to 3 sessions per server, closed after 10 idle minutes, off by default.'
          },
          {
            t: 'File manager',
            d: 'Browse, transfer and edit configs in Monaco inside allowed root paths. Private keys and .env files are blocked by default.'
          },
          {
            t: 'Docker',
            d: 'Container list, live stats, log streaming and lifecycle events, plus start, stop, restart and remove. Off by default.'
          },
          {
            t: 'Upgrades & ops',
            d: 'Upgrade agents right from the panel, SHA-256 verified with automatic rollback if startup fails. Includes cron-scheduled commands when remote exec is enabled.'
          }
        ]
      },
      {
        t: 'Security and access',
        items: [
          {
            t: 'Security events & firewall',
            d: 'Catches SSH brute force and logins from new IPs, plus port scans when enabled, and can drop offending IPs into an nftables blocklist.'
          },
          {
            t: 'Accounts & access',
            d: 'Admin and read-only roles, GitHub, Google, OIDC and Tailscale Serve loopback sign-in, TOTP 2FA and API keys.'
          },
          {
            t: 'Tailscale & Headscale',
            d: 'Enroll agents over your tailnet so the panel never has to sit on the public internet, and check whether each peer is direct or relayed over DERP.'
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
        d: 'One command for Docker or a native binary, with the web UI and SQLite built in so there is no external database to run. Or deploy on Railway.'
      },
      {
        t: 'Enroll a server',
        d: 'Click Add server in the panel and run the generated command on your VPS. The enrollment code works once, expires in 10 minutes, and the agent reconnects on its own after that.'
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
        a: 'No. ServerBee is open source under AGPL-3.0-or-later. You host it on your own machine, with metrics stored in your own SQLite database.'
      },
      {
        q: 'Do monitored servers need open ports?',
        a: 'No. Agents dial out to the server over WebSocket, so monitored hosts need zero inbound ports. Put the server behind HTTPS; pass --domain to the installer to set up Caddy and a certificate automatically.'
      },
      {
        q: 'What do the carrier probes actually measure?',
        a: 'The agent opens TCP connections from your VPS to third-party CDN endpoints in each province for China Telecom, Unicom and Mobile, recording round-trip latency and packet loss. Treat the numbers as route health toward each carrier, not a single user’s broadband speed.'
      },
      {
        q: 'Can I share a public status page?',
        a: 'Yes. The status page is off by default; turn it on to share live health, 90-day uptime, network and IP quality while keeping host IPs masked.'
      },
      {
        q: 'Which platforms are supported?',
        a: 'Static Linux binaries for amd64 and arm64, plus Docker images, for both server and agent. The installer handles systemd and OpenRC. macOS and Windows builds are available (Windows support is basic); security events and firewall blocking are Linux-only.'
      },
      {
        q: 'Where do I get the iOS app?',
        a: 'It’s heading to the App Store. Until then, build it from apps/ios in the repository using XcodeGen and Xcode, signed with your own Apple development team. It runs on iOS {ios} or later.'
      }
    ]
  },
  final: {
    h2: 'Point it at your first server.',
    sub: 'Install the server with one command, paste the enrollment line on a VPS, and watch live metrics roll in.',
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
