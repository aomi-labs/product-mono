import './styles/main.css';
import { Header } from './components/Header';
import { Hero } from './components/Hero';
import { Features } from './components/Features';
import { Footer } from './components/Footer';

class App {
  private container: HTMLElement;

  constructor() {
    this.container = document.getElementById('app')!;
    this.init();
  }

  private init(): void {
    this.render();
    this.attachEventListeners();
  }

  private render(): void {
    this.container.innerHTML = `
      ${Header.render()}
      <main>
        ${Hero.render()}
        ${Features.render()}
      </main>
      ${Footer.render()}
    `;
  }

  private attachEventListeners(): void {
    const tryBotBtn = document.getElementById('try-bot-btn');
    const learnMoreBtn = document.getElementById('learn-more-btn');

    tryBotBtn?.addEventListener('click', () => {
      window.location.href = '/chat';
    });

    learnMoreBtn?.addEventListener('click', () => {
      const featuresSection = document.getElementById('features');
      featuresSection?.scrollIntoView({ behavior: 'smooth' });
    });
  }
}

document.addEventListener('DOMContentLoaded', () => {
  new App();
});