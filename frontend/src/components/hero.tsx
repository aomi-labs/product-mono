"use client";

import { useEffect } from "react";
import Link from "next/link";
import { Settings } from "lucide-react";
import { AomiFrame } from "@aomi-labs/widget-lib";
import { WalletFooter } from "./wallet-footer";
import { useApiKey } from "@/lib/use-api-key";

export const Hero = () => {
  // API key is stored in localStorage and available for future API calls
  // AomiFrame component doesn't currently accept apiKey as a prop
  useApiKey();

  // Scroll reveal animations
  useEffect(() => {
    if (typeof window === "undefined") return;

    const observer = new IntersectionObserver(
      (entries) => {
        entries.forEach((entry) => {
          if (entry.isIntersecting) {
            entry.target.classList.add("animate-in");
            observer.unobserve(entry.target);
          }
        });
      },
      {
        threshold: 0.1,
        rootMargin: "0px 0px -50px 0px",
      }
    );

    const timeoutId = setTimeout(() => {
      document.querySelectorAll(".scroll-reveal, .slide-in-right").forEach((el) => observer.observe(el));
    }, 100);

    return () => {
      clearTimeout(timeoutId);
      observer.disconnect();
    };
  }, []);

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
      <AomiFrame
        height="100%"
        width="100%"
        walletFooter={({ wallet, setWallet }) => (
          <WalletFooter user={wallet} setUser={setWallet} />
        )}
      />
    </div>
  );
};
