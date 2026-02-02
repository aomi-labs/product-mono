/** @type {import('next').NextConfig} */
const nextConfig = {
  reactStrictMode: true,
  output: 'standalone',
  // Allow external images if needed
  images: {
    unoptimized: true,
  },
}
module.exports = nextConfig
