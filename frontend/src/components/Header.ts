export class Header {
  static render(): string {
    return `
      <header>
        <div class="container">
          <nav>
            <a href="/" class="logo">ChatBot</a>
            <ul class="nav-links">
              <li><a href="#home">Home</a></li>
              <li><a href="#features">Features</a></li>
              <li><a href="#about">About</a></li>
              <li><a href="#contact">Contact</a></li>
            </ul>
          </nav>
        </div>
      </header>
    `;
  }
}