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
        <h3 className="text-lg font-semibold text-gray-900 mb-4">API Key</h3>

        {statusMessage && (
          <div
            className={`mb-4 p-3 rounded-md text-sm ${
              statusMessage.type === "success"
                ? "bg-green-50 border border-green-200 text-green-800"
                : "bg-red-50 border border-red-200 text-red-800"
            }`}
          >
            {statusMessage.text}
          </div>
        )}

        <div className="space-y-4">
          <div>
            <label htmlFor="api-key-input" className="block text-sm font-medium text-gray-700 mb-2">
              API Key
            </label>
            <div className="relative">
              <input
                id="api-key-input"
                type={showPassword ? "text" : "password"}
                value={inputValue}
                onChange={(e) => setInputValue(e.target.value)}
                placeholder="Enter your API key"
                className="w-full px-3 py-2 border border-gray-300 rounded-md shadow-sm focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500 text-sm pr-20"
              />
              <button
                type="button"
                onClick={() => setShowPassword(!showPassword)}
                className="absolute right-3 top-1/2 -translate-y-1/2 text-sm text-gray-600 hover:text-gray-900"
              >
                {showPassword ? "Hide" : "Show"}
              </button>
            </div>
            <p className="mt-2 text-sm text-gray-500">
              Your API key is stored locally in your browser and is not shared with anyone.
            </p>
          </div>
        </div>
      </div>

      <div className="flex justify-end gap-3 pt-4 border-t border-gray-200">
        <button
          type="button"
          onClick={handleClear}
          className="px-4 py-2 text-sm font-medium text-gray-700 bg-white border border-gray-300 rounded-md hover:bg-gray-50 focus:outline-none focus:ring-2 focus:ring-gray-300 transition-colors"
        >
          Clear
        </button>
        <button
          type="button"
          onClick={handleSave}
          className="px-4 py-2 text-sm font-medium text-white bg-blue-600 rounded-md hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 transition-colors"
        >
          Save
        </button>
      </div>
    </div>
  );
}
