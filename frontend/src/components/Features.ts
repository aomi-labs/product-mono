export class Features {
  static render(): string {
    return `
      <section id="features" class="features">
        <div class="container">
          <h2>Why Choose Our Chatbot?</h2>
          <div class="features-grid">
            <div class="feature-card">
              <div class="feature-icon">🚀</div>
              <h3>Lightning Fast</h3>
              <p>Get instant responses with our optimized AI engine. No waiting, just immediate assistance whenever you need it.</p>
            </div>
            <div class="feature-card">
              <div class="feature-icon">🧠</div>
              <h3>Intelligent Conversations</h3>
              <p>Advanced natural language processing ensures context-aware discussions that feel natural and engaging.</p>
            </div>
            <div class="feature-card">
              <div class="feature-icon">🔒</div>
              <h3>Privacy First</h3>
              <p>Your conversations are secure and private. We prioritize data protection with enterprise-grade security.</p>
            </div>
            <div class="feature-card">
              <div class="feature-icon">🌐</div>
              <h3>Available 24/7</h3>
              <p>Always ready to help, our chatbot provides round-the-clock assistance for all your questions and needs.</p>
            </div>
            <div class="feature-card">
              <div class="feature-icon">🎯</div>
              <h3>Personalized</h3>
              <p>Learns from your interactions to provide increasingly relevant and helpful responses over time.</p>
            </div>
            <div class="feature-card">
              <div class="feature-icon">⚡</div>
              <h3>Easy to Use</h3>
              <p>Simple, intuitive interface that anyone can use. Just type your question and get helpful answers instantly.</p>
            </div>
          </div>
        </div>
      </section>
    `;
  }
}