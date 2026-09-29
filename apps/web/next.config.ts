import type { NextConfig } from 'next'

const nextConfig: NextConfig = {
  output: 'export',
  reactCompiler: true,
  typedRoutes: true,
  images: {
    unoptimized: true,
  },
}

export default nextConfig
