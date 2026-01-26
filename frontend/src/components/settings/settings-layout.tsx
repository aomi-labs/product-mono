"use client";

import { useState } from "react";
import { SettingsSidebar, SettingsCategory } from "./settings-sidebar";
import { GeneralSettings } from "./general-settings";
import { ApiKeySettings } from "./api-key-settings";
import { PrivacySettings } from "./privacy-settings";
import { AccountSettings } from "./account-settings";
import { CapabilitiesSettings } from "./capabilities-settings";
import { ConnectorsSettings } from "./connectors-settings";
import { BillingSettings } from "./billing-settings";
import Link from "next/link";
import Image from "next/image";
import { ArrowLeft } from "lucide-react";

export function SettingsLayout() {
  const [activeCategory, setActiveCategory] = useState<SettingsCategory>("general");

  const renderContent = () => {
    switch (activeCategory) {
      case "general":
        return <GeneralSettings />;
      case "api-keys":
        return <ApiKeySettings />;
      case "privacy":
        return <PrivacySettings />;
      case "account":
        return <AccountSettings />;
      case "capabilities":
        return <CapabilitiesSettings />;
      case "connectors":
        return <ConnectorsSettings />;
      case "billing":
        return <BillingSettings />;
      default:
        return <GeneralSettings />;
    }
  };

  return (
    <div className="h-screen w-full flex flex-col bg-white">
      {/* Header */}
      <div className="border-b border-gray-200 bg-white px-6 py-4 flex items-center justify-between">
        <div className="flex items-center gap-4">
          <Link
            href="/"
            className="flex items-center gap-2 text-gray-600 hover:text-gray-900 transition-colors"
          >
            <ArrowLeft className="w-5 h-5" />
            <span className="text-sm font-medium">Back to chat</span>
          </Link>
          <div className="h-6 w-px bg-gray-300" />
          <Image src="/assets/images/aomi-logo.svg" alt="Aomi" width={120} height={40} className="h-8 w-auto" />
        </div>
      </div>

      {/* Main Content */}
      <div className="flex-1 flex overflow-hidden">
        <SettingsSidebar activeCategory={activeCategory} onCategoryChange={setActiveCategory} />
        <div className="flex-1 overflow-y-auto p-8">
          <div className="max-w-3xl mx-auto">{renderContent()}</div>
        </div>
      </div>
    </div>
  );
}
