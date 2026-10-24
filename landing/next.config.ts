import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  async redirects() {
    return [{ source: "/", destination: "/en", permanent: false }];
  },
  turbopack: {
    root: process.cwd(),
  },
};

export default nextConfig;
