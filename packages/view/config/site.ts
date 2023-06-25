export type SiteConfig = typeof siteConfig

export const siteConfig = {
  name: "ServerOctopus",
  description:
    "Beautifully designed components built with Radix UI and Tailwind CSS.",
  mainNav: {
    home: {
      title: "Home",
      href: "/",
    },
    console: {
      title: "Console",
      href: "/console",
    },
    login: {
      title: "Login",
      href: "/login",
      hide: true,
    },
    register: {
      title: "Register",
      href: "/register",
      hide: true,
    },
    authentication: {
      title: "Settings",
      href: "/settings",
    },
  },
  links: {
    twitter: "https://twitter.com/zinger_bee",
    github: "https://github.com/ZingerLittleBee/server-octopus",
  },
}
