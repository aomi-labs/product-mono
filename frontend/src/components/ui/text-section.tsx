import React from 'react';
import Image from 'next/image';
import { BlogEntry, TextSectionProps } from '../../lib/types';

export const TextSection: React.FC<TextSectionProps> = ({ type, content, options = {} }) => {
  switch (type) {
    case 'ascii':
      return (
        // https://www.asciiart.eu/text-to-ascii-art Elite
        <div className="ascii-art scroll-reveal scroll-reveal-delay-1 mt-4 mb-5 pr-10 text-center font-mono text-sm text-gray-800 whitespace-pre">
          {content}
        </div>
      );
    case 'ascii-sub':
      return (
        // https://www.asciiart.eu/text-to-ascii-art Elite
        <div className="ascii-art scroll-reveal scroll-reveal-delay-1 pt-10 mt-14 mb-6 text-center font-mono text-[6px] text-gray-800 whitespace-pre">
          {content}
        </div>
      );

    case 'intro-title':
      return (
        <div
          id="about"
          className="scroll-reveal scroll-reveal-delay-1 self-stretch mt-4 mb-12 text-center text-black text-6xl font-bold font-bauhaus leading-[54px] tracking-wide"
        >
          {content}
        </div>
      );

    case 'intro-description':
      return (
        <div className="scroll-reveal scroll-reveal-delay-2 self-stretch mt-2 mb-12 text-left text-justify text-gray-800 text-sm font-light font-dot-gothic tracking-wide">
          {content}
        </div>
      );
    
    case 'h2-title':
      return (
        <h2 className="scroll-reveal scroll-reveal-delay-2 self-stretch mt-10 mb-6 text-center text-gray-900 text-sm font-semibold font-dot-gothic tracking-wide">
          {content}
        </h2>
      );
    case 'paragraph':
      return (
        <li className="scroll-reveal scroll-reveal-delay-2 text-left text-justify text-gray-700 text-sm font-light font-dot-gothic leading-relaxed">
          {content}
        </li>
      );

    default: {
      const className = typeof options.className === 'string' ? options.className : '';
      return (
        <div className={className}>
          {content}
        </div>
      );
    }
  }
};

interface BlogSectionProps {
  blogs: BlogEntry[];
  className?: string;
}

export const BlogSection: React.FC<BlogSectionProps> = ({ blogs, className }) => {
  if (!blogs.length) {
    return null;
  }

  const containerClassName = [
    'self-stretch flex flex-col gap-14',
    className ?? ''
  ].filter(Boolean).join(' ');

  return (
    <div className={containerClassName}>
      {blogs.map((blog, index) => {
        const isEven = index % 2 === 0;

        return (
          <div
            key={`${blog.title}-${index}`}
            className={`scroll-reveal scroll-reveal-delay-2 self-stretch flex flex-col gap-8 md:gap-10 md:items-center ${isEven ? 'md:flex-row' : 'md:flex-row-reverse'}`}
          >
            <div className="w-full md:basis-[35%] md:flex-none">
              <div className="relative w-full overflow-hidden rounded-2xl border border-gray-200 bg-white aspect-[4/3]">
                <Image
                  src={blog.imageSrc}
                  alt={blog.imageAlt}
                  fill
                  className="object-cover"
                  sizes="(min-width: 768px) 320px, 100vw"
                  priority={index === 0}
                />
              </div>
            </div>

            <div
              className={`w-full md:basis-[65%] md:flex-none flex flex-col gap-4 ${isEven ? '' : 'md:items-end md:text-right'}`}
            >
              {blog.eyebrow && (
                <span className={`text-xs uppercase tracking-[0.2em] text-gray-500 font-dot-gothic ${isEven ? '' : 'md:items-end md:text-right'}`}>
                  {blog.eyebrow}
                </span>
              )}
              <h3 className={`text-left text-gray-900 text-xl font-semibold font-bauhaus leading-snug ${isEven ? '' : 'md:text-right'}`}>
                {blog.title}
              </h3>
              <p className={`text-left text-justify text-gray-700 text-sm font-light font-dot-gothic leading-relaxed ${isEven ? '' : 'md:text-right'}`}>
                {blog.description}
              </p>
              {blog.cta && (
                <a
                  href={blog.cta.href}
                  className={`text-sm font-medium text-gray-900 underline underline-offset-4 ${isEven ? '' : 'md:text-right'}`}
                  target="_blank"
                  rel="noreferrer"
                >
                  {blog.cta.label}
                </a>
              )}
            </div>
          </div>
        );
      })}
    </div>
  );
};
