'use client';

import dynamic from 'next/dynamic';

const SignTransaction = dynamic(() => import('@/components/sign-transaction'), {
  ssr: false,
  loading: () => (
    <main className="min-h-screen bg-gradient-to-b from-gray-900 to-black text-white p-6 flex items-center justify-center">
      <div className="text-center">
        <div className="animate-spin rounded-full h-12 w-12 border-t-2 border-b-2 border-white mx-auto mb-4"></div>
        <p className="text-gray-400">Loading transaction...</p>
      </div>
    </main>
  ),
});

export default function SignPage() {
  return <SignTransaction />;
}
