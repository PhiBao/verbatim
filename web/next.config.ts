import { NextConfig } from "next";

const nextConfig: NextConfig = {
  // The engine owns every /api route. Next.js rewrites to it so the browser has
  // a single origin and CORS never enters the picture.
  async rewrites() {
    const engine = process.env.ENGINE_URL ?? "http://127.0.0.1:8080";
    return [{ source: "/api/:path*", destination: `${engine}/api/:path*` }];
  },
};

export default nextConfig;
