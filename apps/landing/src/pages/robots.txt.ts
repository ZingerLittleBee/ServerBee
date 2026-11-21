import type { APIRoute } from 'astro'

/** Built from the configured site, so a deployment on another origin points crawlers at its own sitemap. */
export const GET: APIRoute = ({ site }) =>
  new Response(`User-agent: *\nAllow: /\n\nSitemap: ${new URL('sitemap-index.xml', site)}\n`, {
    headers: { 'content-type': 'text/plain; charset=utf-8' }
  })
