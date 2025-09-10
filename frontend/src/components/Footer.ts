export class Footer {
  static render(): string {
    return `
      <footer>
        <div class="container">
          <p>&copy; ${new Date().getFullYear()} ChatBot. Built with ❤️ for better conversations.</p>
        </div>
      </footer>
    `;
  }
}