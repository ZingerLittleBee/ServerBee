import {
  kClientRoute,
  kConsoleRoute,
  kDashboardRoute,
  kLoginRoute,
  kRegisterRoute,
  kSettingsRoute,
} from "@/lib/route"

export type SiteConfig = typeof siteConfig

export const siteConfig = {
  name: "ServerOctopus",
  description:
    "Beautifully designed components built with Radix UI and Tailwind CSS.",
  mainNav: {
    // home: {
    //   title: "Home",
    //   href: "/",
    // },
    dashboard: {
      title: "Dashboard",
      href: kDashboardRoute,
    },
    client: {
      title: "Client",
      href: kClientRoute,
    },
    console: {
      title: "Console",
      href: kConsoleRoute,
    },
    login: {
      title: "Login",
      href: kLoginRoute,
      hide: true,
    },
    register: {
      title: "Register",
      href: kRegisterRoute,
      hide: true,
    },
    authentication: {
      title: "Settings",
      href: kSettingsRoute,
    },
  },
  links: {
    twitter: "https://twitter.com/zinger_bee",
    github: "https://github.com/ZingerLittleBee/server-octopus",
  },
}
