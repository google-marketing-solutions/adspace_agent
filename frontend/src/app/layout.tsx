import type { Metadata } from "next";
import { Geist, Geist_Mono } from "next/font/google";
import "./globals.css";
import Providers from "./providers";
import { AccountProvider } from "@/components/AccountContext";
import { AppSidebar } from "@/components/layout/AppSidebar";
import { CommandPalette } from "@/components/layout/CommandPalette";
import { SidebarInset, SidebarProvider } from "@/components/ui/sidebar";

const geistSans = Geist({
  variable: "--font-geist-sans",
  subsets: ["latin"],
});

const geistMono = Geist_Mono({
  variable: "--font-geist-mono",
  subsets: ["latin"],
});

export const metadata: Metadata = {
  title: "AdSpace Agent",
  description: "AI-powered Google Ads management dashboard",
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en" suppressHydrationWarning>
      <body
        className={`${geistSans.variable} ${geistMono.variable} antialiased`}
      >
        <Providers>
          <AccountProvider>
            <SidebarProvider>
              <AppSidebar />
              <SidebarInset>
                <main className="flex-1 overflow-y-auto">
                  <div className="p-6 max-w-[1600px] mx-auto">{children}</div>
                </main>
              </SidebarInset>
            </SidebarProvider>
            <CommandPalette />
          </AccountProvider>
        </Providers>
      </body>
    </html>
  );
}
