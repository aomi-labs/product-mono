// Type definitions for the application

export interface TextSectionProps {
  type: 'ascii' | 'intro-title' | 'intro-description' | 'h2-title' | 'paragraph' | 'ascii-sub' | 'headline';
  content: string;
  options?: Record<string, unknown>;
}

export interface BlogEntry {
  slug: string;
  title: string;
  description: string;
  imageSrc: string;
  imageAlt: string;
  eyebrow?: string;
  publishedAt?: string;
  cta?: {
    label: string;
    href: string;
  };
  body?: string;
}
