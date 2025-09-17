import type { Metadata } from "next";
import { Inter } from "next/font/google";
import "./globals.css";
import { Providers } from "@/components/providers";

const inter = Inter({ subsets: ["latin"] });

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
      <head>
        {/* Google Fonts */}
        <link rel="preconnect" href="https://fonts.googleapis.com" />
        <link rel="preconnect" href="https://fonts.gstatic.com" crossOrigin="" />
        <link
          href="https://fonts.googleapis.com/css2?family=DM+Sans:ital,opsz,wght@0,9..40,100..1000;1,9..40,100..1000&family=Inter:wght@100..900&family=Asap:ital,wght@0,100..900;1,100..900&family=Roboto+Mono:ital,wght@0,100..700;1,100..700&family=Source+Code+Pro:wght@300;400;500;600;700;800;900&display=swap"
          rel="stylesheet"
        />
        <link href="https://fonts.googleapis.com/css2?family=DotGothic16&display=swap" rel="stylesheet" />

        {/* Custom Fonts */}
        <style dangerouslySetInnerHTML={{
          __html: `
            @font-face {
              font-family: 'Bauhaus_Chez_Display_2.0';
              src: url('/assets/fonts/BauhausChezDisplay2.0-Regular.otf') format('opentype');
              font-weight: normal;
              font-style: normal;
              font-display: swap;
            }
            @font-face {
              font-family: 'Bauhaus_Chez_Display_2.0';
              src: url('/assets/fonts/BauhausChezDisplay2.0-Light.otf') format('opentype');
              font-weight: 300;
              font-style: normal;
              font-display: swap;
            }
            @font-face {
              font-family: 'Bauhaus_Chez_Display_2.0';
              src: url('/assets/fonts/BauhausChezDisplay2.0-Medium.otf') format('opentype');
              font-weight: 500;
              font-style: normal;
              font-display: swap;
            }
            @font-face {
              font-family: 'Bauhaus_Chez_Display_2.0';
              src: url('/assets/fonts/BauhausChezDisplay2.0-SemiBold.otf') format('opentype');
              font-weight: 600;
              font-style: normal;
              font-display: swap;
            }
            @font-face {
              font-family: 'Bauhaus_Chez_Display_2.0';
              src: url('/assets/fonts/BauhausChezDisplay2.0-Bold.otf') format('opentype');
              font-weight: 700;
              font-style: normal;
              font-display: swap;
            }
            @font-face {
              font-family: 'Bauhaus_Chez_Display_2.0';
              src: url('/assets/fonts/BauhausChezDisplay2.0-Black.otf') format('opentype');
              font-weight: 900;
              font-style: normal;
              font-display: swap;
            }
          `
        }} />
      </head>
      <body className={inter.className}>
        <Providers>
          <div className="relative min-h-screen">
            {children}
          </div>
        </Providers>
      </body>
    </html>
  );
}
