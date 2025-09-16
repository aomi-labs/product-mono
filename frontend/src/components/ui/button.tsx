import React from 'react';
import { ButtonProps } from '../../lib/types';

export const Button: React.FC<ButtonProps> = ({
  children,
  variant = 'default',
  onClick,
  showIndicator = false,
  disabled = false,
  className = '',
}) => {
  const baseClass = 'text-sm rounded-[4px] border transition-colors';

  const variants = {
    'tab-inactive': 'px-3 py-0.5 w-[130px] h-6 bg-gray-700 text-gray-300 text-xs border-gray-600 border-0.2 hover:bg-gray-600 hover:text-white',
    'tab-active': 'px-3 py-0.5 w-[130px] h-6 bg-gray-500 text-white text-xs border-gray-500 border-0.2 hover:bg-gray-500',
    'github': 'px-4 py-3 bg-black text-white rounded-full hover:bg-gray-800',
    'terminal-connect': 'bg-green-600 text-white px-3 py-1 text-xs rounded-lg border-0 h-6 hover:bg-green-500',
    'default': 'px-4 py-2 bg-blue-500 text-white hover:bg-blue-600',
  };

  const classes = `${baseClass} ${variants[variant]} ${className} ${disabled ? 'opacity-50 cursor-not-allowed' : 'cursor-pointer'}`;

  const handleClick = () => {
    if (!disabled && onClick) {
      onClick();
    }
  };

  if (variant === 'tab-active' && showIndicator) {
    return (
      <button className={classes} onClick={handleClick} disabled={disabled}>
        <span className="flex items-center gap-1">
          <span className="w-2 h-2 bg-green-400 rounded-full"></span>
          <span>{children}</span>
        </span>
      </button>
    );
  }

  return (
    <button className={classes} onClick={handleClick} disabled={disabled}>
      {children}
    </button>
  );
};