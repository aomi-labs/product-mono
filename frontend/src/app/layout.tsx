import type { Metadata } from "next";
import { Geist, Geist_Mono } from "next/font/google";
import localFont from "next/font/local";
import { cookies } from "next/headers";
import "./globals.css";
import { WalletProviders } from "@/components/wallet-providers";

const geistSans = Geist({
  variable: "--font-geist-sans",
  subsets: ["latin"],
});

const geistMono = Geist_Mono({
  variable: "--font-geist-mono",
  subsets: ["latin"],
});

const iaWriterMono = localFont({
  src: [
    {
      path: "../../public/assets/fonts/iAWriterMonoS-Regular.ttf",
      weight: "400",
      style: "normal",
    },
  ],
  variable: "--font-ia-writer",
});

export const metadata: Metadata = {
  title: "Aomi Labs",
  description: "A research and engineering group focused on building agentic software for blockchain automation",
  icons: {
    icon: "/assets/images/a.svg",
    shortcut: "/assets/images/a.svg",
    apple: "/assets/images/a.svg",
  },
};

export default async function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  const cookieStore = await cookies();
  const cookieString = cookieStore
    .getAll()
    .map(({ name, value }) => `${name}=${value}`)
    .join("; ");

  return (
    <html lang="en">
      <body className={`${geistSans.variable} ${geistMono.variable} ${iaWriterMono.variable} antialiased`}>
        <WalletProviders cookies={cookieString || null}>
          <div className="relative min-h-screen">{children}</div>
        </WalletProviders>
      </body>
    </html>
  );
}
