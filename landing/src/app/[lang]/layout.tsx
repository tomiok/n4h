import type { Metadata } from "next";
import { notFound } from "next/navigation";
import { dictionaries, isLocale } from "@/lib/i18n";
import "../globals.css";

type LayoutProps = {
  children: React.ReactNode;
  params: Promise<{ lang: string }>;
};

export async function generateMetadata({
  params,
}: LayoutProps): Promise<Metadata> {
  const { lang } = await params;
  if (!isLocale(lang)) notFound();
  const copy = dictionaries[lang];
  const siteUrl =
    process.env.NEXT_PUBLIC_SITE_URL ||
    (process.env.VERCEL_PROJECT_PRODUCTION_URL
      ? `https://${process.env.VERCEL_PROJECT_PRODUCTION_URL}`
      : "http://localhost:3000");

  return {
    metadataBase: new URL(siteUrl),
    title: "n4h — Not for Human",
    description: copy.metadata.description,
    alternates: {
      canonical: `/${lang}`,
      languages: { en: "/en", es: "/es", "x-default": "/en" },
    },
    openGraph: {
      title: "n4h — Not for Human",
      description: copy.metadata.description,
      url: `/${lang}`,
      locale: copy.metadata.ogLocale,
      alternateLocale: [
        dictionaries[lang === "en" ? "es" : "en"].metadata.ogLocale,
      ],
      type: "website",
    },
  };
}

export default async function RootLayout({
  children,
  params,
}: Readonly<LayoutProps>) {
  const { lang } = await params;
  if (!isLocale(lang)) notFound();

  return (
    <html lang={lang}>
      <body>{children}</body>
    </html>
  );
}
