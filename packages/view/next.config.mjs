/** @type {import('next').NextConfig} */
const nextConfig = {
    async rewrites() {
        return [
            {
                source: '/api/s/:path*',
                destination: 'http://localhost:3001/:path*',
            },
            {
                source: '/api/c/:path*',
                destination: 'http://localhost:3002/:path*',
            },
        ]
    }
}

export default nextConfig
