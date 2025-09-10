import axios from 'axios';
import { ApiResponse } from '@/types';

const api = axios.create({
  baseURL: process.env.NODE_ENV === 'production' ? '/api' : 'http://localhost:8080/api',
  timeout: 10000,
  headers: {
    'Content-Type': 'application/json',
  },
});

export const sendMessage = async (message: string): Promise<ApiResponse<string>> => {
  try {
    const response = await api.post('/chat', { message });
    return {
      success: true,
      data: response.data.response,
    };
  } catch (error) {
    return {
      success: false,
      error: error instanceof Error ? error.message : 'Unknown error occurred',
    };
  }
};

export default api;