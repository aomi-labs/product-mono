export class Hero {
  static render(): string {
    return `
      <section class="hero">
        <div class="container">
          <h1>Meet Your AI Assistant</h1>
          <p>Experience the future of conversation with our advanced chatbot. Get instant answers, creative solutions, and engaging discussions powered by cutting-edge AI technology.</p>
          <div style="display: flex; gap: 1rem; justify-content: center; flex-wrap: wrap;">
            <button id="try-bot-btn" class="btn btn-primary">Try Now</button>
            <button id="learn-more-btn" class="btn btn-secondary">Learn More</button>
          </div>
        </div>
      </section>
    `;
  }
}