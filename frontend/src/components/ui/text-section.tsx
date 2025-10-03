import React from 'react';
import { TextSectionProps } from '../../lib/types';

export const TextSection: React.FC<TextSectionProps> = ({ type, content, options = {} }) => {
  switch (type) {
    case 'ascii':
      return (
        // https://www.asciiart.eu/text-to-ascii-art Elite
        <div className="ascii-art scroll-reveal scroll-reveal-delay-1 mt-4 mb-10 pr-10 text-center font-mono text-sm text-gray-800 whitespace-pre">
          {content}
        </div>
      );
    case 'ascii-sub':
      return (
        // https://www.asciiart.eu/text-to-ascii-art Elite
        <div className="ascii-art scroll-reveal scroll-reveal-delay-1 pt-10 mt-14 mb-4 text-center font-mono text-[6px] text-gray-800 whitespace-pre">
          {content}
        </div>
      );

    case 'intro-title':
      return (
        <div
          id="about"
          className="scroll-reveal scroll-reveal-delay-1 self-stretch mt-4 mb-12 text-center text-black text-6xl font-bold font-bauhaus leading-[54px]"
        >
          {content}
        </div>
      );

    case 'intro-description':
      return (
        <div className="scroll-reveal scroll-reveal-delay-2 self-stretch mt-2 mb-12 text-left text-gray-800 text-sm font-light font-dot-gothic tracking-wide">
          {content}
        </div>
      );
    
    case 'h2-title':
      return (
        <h2 className="scroll-reveal scroll-reveal-delay-2 self-stretch mt-10 mb-6 text-center text-gray-900 text-xl font-semibold font-bauhaus">
          {content}
        </h2>
      );
    case 'paragraph':
      return (
        <li className="scroll-reveal scroll-reveal-delay-2 text-left text-gray-700 text-sm font-light font-dot-gothic leading-relaxed">
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
