"use client";

import { useEffect, useRef, useState } from "react";
import Image from "next/image";
import { AomiFrame } from "@aomi-labs/widget-lib";
import { WalletFooter } from "./wallet-footer";
import { BlogSection, TextSection } from "./ui/text-section";
import { content, bodies, blogs } from "./content";

export const Hero = () => {
  const [terminalState, setTerminalState] = useState<"normal" | "minimized" | "expanded" | "closed">("normal");
  const [lastOpenState, setLastOpenState] = useState<"normal" | "expanded">("normal");
  const [isMinimizing, setIsMinimizing] = useState(false);
  const [isRestoringFromMinimize, setIsRestoringFromMinimize] = useState(false);
  const minimizeTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const restoreTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

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

  // Cleanup timeouts on unmount
  useEffect(() => {
    return () => {
      if (minimizeTimeoutRef.current) {
        clearTimeout(minimizeTimeoutRef.current);
      }
      if (restoreTimeoutRef.current) {
        clearTimeout(restoreTimeoutRef.current);
      }
    };
  }, []);

  const handleTerminalClose = () => {
    if (minimizeTimeoutRef.current) {
      clearTimeout(minimizeTimeoutRef.current);
      minimizeTimeoutRef.current = null;
    }
    if (restoreTimeoutRef.current) {
      clearTimeout(restoreTimeoutRef.current);
      restoreTimeoutRef.current = null;
    }
    setIsMinimizing(false);
    setIsRestoringFromMinimize(false);
    setLastOpenState("normal");
    setTerminalState("closed");
  };

  const handleTerminalMinimize = () => {
    if (terminalState === "minimized" || terminalState === "closed") return;
    if (minimizeTimeoutRef.current) {
      clearTimeout(minimizeTimeoutRef.current);
    }
    if (restoreTimeoutRef.current) {
      clearTimeout(restoreTimeoutRef.current);
      restoreTimeoutRef.current = null;
    }
    setIsRestoringFromMinimize(false);
    setLastOpenState(terminalState === "expanded" ? "expanded" : "normal");
    setIsMinimizing(true);
    minimizeTimeoutRef.current = setTimeout(() => {
      setIsMinimizing(false);
      setTerminalState("minimized");
      minimizeTimeoutRef.current = null;
    }, 230);
  };

  const handleTerminalExpand = () => {
    setTerminalState((prev) => {
      const next = prev === "expanded" ? "normal" : "expanded";
      setLastOpenState(next as "normal" | "expanded");
      return next;
    });
  };

  const handleRestoreFromClosed = () => {
    if (minimizeTimeoutRef.current) {
      clearTimeout(minimizeTimeoutRef.current);
      minimizeTimeoutRef.current = null;
    }
    setLastOpenState("normal");
    setTerminalState("normal");
  };

  const handleRestoreFromMinimized = () => {
    if (restoreTimeoutRef.current) {
      clearTimeout(restoreTimeoutRef.current);
    }
    setIsMinimizing(false);
    setTerminalState(lastOpenState);
    setIsRestoringFromMinimize(true);
    restoreTimeoutRef.current = setTimeout(() => {
      setIsRestoringFromMinimize(false);
      restoreTimeoutRef.current = null;
    }, 350);
  };

  const isTerminalVisible = terminalState !== "closed" && terminalState !== "minimized";
  const terminalWrapperSpacing = terminalState === "closed" || terminalState === "minimized" ? "pt-4 pb-6" : "pt-10 pb-10";
  const terminalSizeClasses = terminalState === "expanded" ? "max-w-[1260px] h-[900px]" : "max-w-[900px] h-[680px]";
  const terminalAnimationClass = isMinimizing ? "terminal-animate-shrink" : isRestoringFromMinimize ? "terminal-animate-pop" : "";

  return (
    <div id="main-container" className="w-full flex px-10 pb-5 relative bg-white flex flex-col justify-start items-center overflow-hidden">
      <div data-breakpoint="Desktop" className="self-stretch flex flex-col justify-start items-center">
        <div className="desktop-nav w-full h-26 flex pt-5 pb-5 flex justify-between items-center px-4">
          <Image src="/assets/images/aomi-logo.svg" alt="Aomi" width={200} height={72} className="h-15 w-auto" priority />
          <a
            href="https://github.com/aomi-labs"
            target="_blank"
            rel="noopener noreferrer"
            className="px-4 py-3 bg-black rounded-full flex justify-center items-center gap-0.5 hover:bg-gray-800"
          >
            <div className="text-center justify-start pt-1 text-white text-sm font-light font-['Bauhaus_Chez_Display_2.0'] leading-tight">
              Github ↗
            </div>
          </a>
        </div>
      </div>

      <div className={`w-full max-w-[1500px] flex flex-col justify-start items-center ${terminalWrapperSpacing}`}>
        {isTerminalVisible && (
          <div
            id="terminal-container"
            className={`w-full ${terminalSizeClasses} flex flex-col bg-white rounded-2xl shadow-[0_25px_50px_-12px_rgba(0,0,0,0.15)] border border-gray-200 transition-all duration-300 transform origin-bottom-left ${terminalAnimationClass}`}
          >
            {/* Terminal Header with window controls */}
            <div className="terminal-header bg-gray-50/50 backdrop-blur-sm px-5 py-3 flex items-center justify-between border-b border-gray-100">
              <div className="flex items-center space-x-4">
                <div className="flex space-x-2">
                  <button
                    type="button"
                    aria-label="Close terminal"
                    onClick={handleTerminalClose}
                    className="w-3 h-3 bg-[#FF5F57] rounded-full hover:bg-[#FF5F57]/80 transition-colors focus:outline-none focus:ring-2 focus:ring-red-200"
                  />
                  <button
                    type="button"
                    aria-label="Minimize terminal"
                    onClick={handleTerminalMinimize}
                    className="w-3 h-3 bg-[#FEBC2E] rounded-full hover:bg-[#FEBC2E]/80 transition-colors focus:outline-none focus:ring-2 focus:ring-yellow-200"
                  />
                  <button
                    type="button"
                    aria-label="Expand terminal"
                    onClick={handleTerminalExpand}
                    className="w-3 h-3 bg-[#28C840] rounded-full hover:bg-[#28C840]/80 transition-colors focus:outline-none focus:ring-2 focus:ring-green-200"
                  />
                </div>
              </div>
            </div>

            {/* AomiFrame Chat Widget */}
            <div className="flex-1 w-full relative overflow-hidden rounded-b-2xl">
              <AomiFrame
                height="100%"
                width="100%"
                walletFooter={(props) => <WalletFooter {...props} />}
              />
            </div>
          </div>
        )}

        {terminalState === "closed" && (
          <div className="py-10">
            <button
              type="button"
              onClick={handleRestoreFromClosed}
              className="px-8 py-3 rounded-full bg-gray-200 text-gray-900 text-sm font-light font-['Bauhaus_Chez_Display_2.0'] hover:bg-gray-300 transition-colors border border-gray-300"
            >
              + New Conversation
            </button>
          </div>
        )}
      </div>

      {terminalState === "minimized" && (
        <button
          type="button"
          aria-label="Restore terminal"
          onClick={handleRestoreFromMinimized}
          className="fixed bottom-6 left-6 h-14 w-14 rounded-full bg-gray-900 text-white flex items-center justify-center shadow-xl border border-gray-700 focus:outline-none focus:ring-2 focus:ring-gray-400"
        >
          <span className="text-2xl">💬</span>
        </button>
      )}
    </div>
  );
};
