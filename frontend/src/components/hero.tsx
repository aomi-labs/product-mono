"use client";

import Link from "next/link";
import { Settings } from "lucide-react";
import { AomiFrame } from "@aomi-labs/widget-lib";

export const Hero = () => {
  return (
    <div className="h-screen w-full bg-white overflow-hidden relative">
      {/* Settings Button - Top Right */}
      <Link
        href="/settings"
        className="fixed top-4 right-4 z-50 p-2 text-gray-600 hover:text-gray-900 hover:bg-gray-100 rounded transition-colors focus:outline-none focus:ring-2 focus:ring-gray-300 bg-white/80 backdrop-blur-sm shadow-sm"
        aria-label="Open settings"
      >
        <Settings className="w-5 h-5" />
      </Link>

      {/* Full-Screen Chat Container */}
      <AomiFrame.Root height="100%" width="100%" walletPosition="footer">
        <AomiFrame.Header withControl />
        <AomiFrame.Composer />
      </AomiFrame.Root>
    </div>
  );
};
