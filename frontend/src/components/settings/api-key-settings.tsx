"use client";

import { useState, useEffect } from "react";
import { useApiKey } from "@/lib/use-api-key";

export function ApiKeySettings() {
  const { apiKey, setApiKey } = useApiKey();
  const [inputValue, setInputValue] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [statusMessage, setStatusMessage] = useState<{ type: "success" | "error"; text: string } | null>(null);

  useEffect(() => {
    setInputValue(apiKey);
  }, [apiKey]);

  const handleSave = () => {
    setApiKey(inputValue);
    setStatusMessage({ type: "success", text: "API key saved successfully." });
    setTimeout(() => {
      setStatusMessage(null);
    }, 3000);
  };

  const handleClear = () => {
    setInputValue("");
    setApiKey("");
    setStatusMessage({ type: "success", text: "API key cleared successfully." });
    setTimeout(() => {
      setStatusMessage(null);
    }, 3000);
  };

  return (
    <div className="space-y-6">
      <div>
        <h3 className="text-lg font-semibold text-foreground mb-4">API Key</h3>

        {statusMessage && (
          <div
            className={`mb-4 p-3 rounded-md text-sm ${
              statusMessage.type === "success"
                ? "bg-green-500/10 border border-green-500/20 text-green-600 dark:text-green-400"
                : "bg-destructive/10 border border-destructive/20 text-destructive"
            }`}
          >
            {statusMessage.text}
          </div>
        )}

        <div className="space-y-4">
          <div>
            <label htmlFor="api-key-input" className="block text-sm font-medium text-foreground mb-2">
              API Key
            </label>
            <div className="relative">
              <input
                id="api-key-input"
                type={showPassword ? "text" : "password"}
                value={inputValue}
                onChange={(e) => setInputValue(e.target.value)}
                placeholder="Enter your API key"
                className="w-full px-5 py-3 border border-input rounded-full shadow-sm focus:outline-none focus:ring-2 focus:ring-ring focus:border-ring text-sm pr-20 bg-background text-foreground"
              />
              <button
                type="button"
                onClick={() => setShowPassword(!showPassword)}
                className="absolute right-3 top-1/2 -translate-y-1/2 text-sm text-muted-foreground hover:text-foreground"
              >
                {showPassword ? "Hide" : "Show"}
              </button>
            </div>
            <p className="mt-2 text-sm text-muted-foreground">
              Your API key is stored locally in your browser and is not shared with anyone.
            </p>
          </div>
        </div>
      </div>

      <div className="flex justify-end gap-3 pt-4 border-t border-border">
        <button
          type="button"
          onClick={handleClear}
          className="px-6 py-3 text-sm font-medium text-muted-foreground bg-background border border-input rounded-full hover:bg-accent focus:outline-none focus:ring-2 focus:ring-ring transition-colors"
        >
          Clear
        </button>
        <button
          type="button"
          onClick={handleSave}
          className="px-6 py-3 text-sm font-medium text-primary-foreground bg-primary rounded-full hover:bg-primary/90 focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 focus:ring-offset-background transition-colors"
        >
          Save
        </button>
      </div>
    </div>
  );
}
