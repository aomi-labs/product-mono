import React from 'react';
import { TextSectionProps } from '../../lib/types';

export const TextSection: React.FC<TextSectionProps> = ({ type, content, options = {} }) => {
  switch (type) {
    case 'ascii':
      return (
        <div className="ascii-art scroll-reveal scroll-reveal-delay-1 text-center font-mono text-sm text-gray-800 whitespace-pre">
          {content}
        </div>
      );

    case 'intro-title':
      return (
        <div
          id="about"
          className="scroll-reveal scroll-reveal-delay-1 self-stretch text-center text-black text-6xl font-bold font-bauhaus leading-[54px]"
        >
          {content}
        </div>
      );

    case 'intro-description':
      return (
        <div className="scroll-reveal scroll-reveal-delay-2 self-stretch text-left text-gray-800 text-sm font-light font-dot-gothic tracking-wide">
          {content}
        </div>
      );

    default:
      return (
        <div className={options.className || ''}>
          {content}
        </div>
      );
  }
};
