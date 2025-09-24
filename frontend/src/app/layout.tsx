import type { Metadata } from "next";
import { Inter, DotGothic16, Source_Code_Pro } from "next/font/google";
import "./globals.css";
import { Providers } from "@/components/providers";

const inter = Inter({ subsets: ["latin"], variable: "--font-inter" });
const dotGothic = DotGothic16({ weight: "400", subsets: ["latin"], variable: "--font-dot-gothic" });
const sourceCodePro = Source_Code_Pro({
  subsets: ["latin"],
  weight: ["300", "400", "500", "600", "700", "800", "900"],
  variable: "--font-source-code",
});

export const metadata: Metadata = {
  title: "aomi labs",
  description: "A research and engineering group focused on building agentic software for blockchain automation",
  icons: {
    icon: '/assets/images/a.svg',
    shortcut: '/assets/images/a.svg',
    apple: '/assets/images/a.svg',
  },
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en">
      <body className={`${inter.variable} ${dotGothic.variable} ${sourceCodePro.variable}`}>
        <Providers>
          <div className="relative min-h-screen">
            {children}
          </div>
        </Providers>
      </body>
    </html>
  );
}
