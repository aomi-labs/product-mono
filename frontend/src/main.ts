import './styles/terminal.css';
import { TerminalLanding } from './components/TerminalLanding';

class App {
  private container: HTMLElement;

  constructor() {
    this.container = document.getElementById('app')!;
    this.init();
  }

  private init(): void {
    new TerminalLanding(this.container);
  }
}

document.addEventListener('DOMContentLoaded', () => {
  new App();
});