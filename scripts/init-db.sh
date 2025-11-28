#!/bin/bash
set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
DB_USER="aomi"
DB_PASSWORD="aomi1234"
DB_NAME="chatbot"
DB_PORT="5432"
DB_HOST="localhost"

echo -e "${GREEN}=== PostgreSQL Database Initialization Script ===${NC}"
echo ""

# Function to check if PostgreSQL is installed
check_postgres() {
    if command -v psql &> /dev/null; then
        echo -e "${GREEN}✓${NC} PostgreSQL is already installed"
        return 0
    else
        echo -e "${YELLOW}!${NC} PostgreSQL is not installed"
        return 1
    fi
}

# Function to install PostgreSQL
install_postgres() {
    echo -e "${YELLOW}Installing PostgreSQL...${NC}"

    if [[ "$OSTYPE" == "darwin"* ]]; then
        # macOS
        if command -v brew &> /dev/null; then
            echo "Installing PostgreSQL via Homebrew..."
            brew install postgresql@14
            echo "Starting PostgreSQL service..."
            brew services start postgresql@14

            # Add PostgreSQL to PATH
            export PATH="/opt/homebrew/opt/postgresql@14/bin:$PATH"

            # Wait for PostgreSQL to start
            echo "Waiting for PostgreSQL to start..."
            sleep 3
        else
            echo -e "${RED}Error: Homebrew is not installed. Please install Homebrew first.${NC}"
            echo "Visit: https://brew.sh"
            exit 1
        fi
    elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
        # Linux
        if command -v apt-get &> /dev/null; then
            echo "Installing PostgreSQL via apt..."
            sudo apt-get update
            sudo apt-get install -y postgresql postgresql-contrib
            sudo systemctl start postgresql
            sudo systemctl enable postgresql
        elif command -v yum &> /dev/null; then
            echo "Installing PostgreSQL via yum..."
            sudo yum install -y postgresql-server postgresql-contrib
            sudo postgresql-setup initdb
            sudo systemctl start postgresql
            sudo systemctl enable postgresql
        else
            echo -e "${RED}Error: Unsupported Linux distribution${NC}"
            exit 1
        fi
    else
        echo -e "${RED}Error: Unsupported operating system${NC}"
        exit 1
    fi

    echo -e "${GREEN}✓${NC} PostgreSQL installed successfully"
}

# Function to check if user exists
user_exists() {
    if [[ "$OSTYPE" == "linux-gnu"* ]]; then
        sudo -u postgres psql -tAc "SELECT 1 FROM pg_roles WHERE rolname='$DB_USER'" | grep -q 1
    else
        psql postgres -tAc "SELECT 1 FROM pg_roles WHERE rolname='$DB_USER'" 2>/dev/null | grep -q 1
    fi
}

# Function to check if database exists
db_exists() {
    if [[ "$OSTYPE" == "linux-gnu"* ]]; then
        sudo -u postgres psql -tAc "SELECT 1 FROM pg_database WHERE datname='$DB_NAME'" | grep -q 1
    else
        psql postgres -tAc "SELECT 1 FROM pg_database WHERE datname='$DB_NAME'" 2>/dev/null | grep -q 1
    fi
}

# Function to setup database user and database
setup_database() {
    echo ""
    echo -e "${YELLOW}Setting up database user and database...${NC}"

    # Create user if it doesn't exist
    if user_exists; then
        echo -e "${GREEN}✓${NC} User '$DB_USER' already exists"
    else
        echo "Creating user '$DB_USER'..."
        if [[ "$OSTYPE" == "linux-gnu"* ]]; then
            sudo -u postgres psql -c "CREATE USER $DB_USER WITH PASSWORD '$DB_PASSWORD';"
        else
            psql postgres -c "CREATE USER $DB_USER WITH PASSWORD '$DB_PASSWORD';"
        fi
        echo -e "${GREEN}✓${NC} User '$DB_USER' created"
    fi

    # Create database if it doesn't exist
    if db_exists; then
        echo -e "${GREEN}✓${NC} Database '$DB_NAME' already exists"
    else
        echo "Creating database '$DB_NAME'..."
        if [[ "$OSTYPE" == "linux-gnu"* ]]; then
            sudo -u postgres psql -c "CREATE DATABASE $DB_NAME OWNER $DB_USER;"
        else
            psql postgres -c "CREATE DATABASE $DB_NAME OWNER $DB_USER;"
        fi
        echo -e "${GREEN}✓${NC} Database '$DB_NAME' created"
    fi

    # Grant privileges
    echo "Granting privileges..."
    if [[ "$OSTYPE" == "linux-gnu"* ]]; then
        sudo -u postgres psql -c "GRANT ALL PRIVILEGES ON DATABASE $DB_NAME TO $DB_USER;"
        sudo -u postgres psql -d $DB_NAME -c "GRANT ALL ON SCHEMA public TO $DB_USER;"
    else
        psql postgres -c "GRANT ALL PRIVILEGES ON DATABASE $DB_NAME TO $DB_USER;"
        psql $DB_NAME -c "GRANT ALL ON SCHEMA public TO $DB_USER;"
    fi
    echo -e "${GREEN}✓${NC} Privileges granted"
}

# Function to check if sqlx-cli is installed
check_sqlx_cli() {
    if command -v sqlx &> /dev/null; then
        echo -e "${GREEN}✓${NC} sqlx-cli is already installed"
        return 0
    else
        echo -e "${YELLOW}!${NC} sqlx-cli is not installed"
        return 1
    fi
}

# Function to install sqlx-cli
install_sqlx_cli() {
    echo -e "${YELLOW}Installing sqlx-cli...${NC}"
    cargo install sqlx-cli --no-default-features --features postgres
    echo -e "${GREEN}✓${NC} sqlx-cli installed successfully"
}

# Function to run migrations
run_migrations() {
    echo ""
    echo -e "${YELLOW}Running database migrations...${NC}"

    # Get the script directory and project root
    SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"
    PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
    MIGRATIONS_DIR="$PROJECT_ROOT/aomi/bin/backend"

    if [ ! -d "$MIGRATIONS_DIR/migrations" ]; then
        echo -e "${RED}Error: Migrations directory not found at $MIGRATIONS_DIR/migrations${NC}"
        exit 1
    fi

    cd "$MIGRATIONS_DIR"

    # Export DATABASE_URL for sqlx
    export DATABASE_URL="postgres://$DB_USER:$DB_PASSWORD@$DB_HOST:$DB_PORT/$DB_NAME"

    # Run migrations
    sqlx migrate run

    echo -e "${GREEN}✓${NC} Migrations completed successfully"
    cd "$PROJECT_ROOT"
}

# Function to setup environment variable
setup_env() {
    echo ""
    echo -e "${YELLOW}Setting up environment variable...${NC}"

    export DATABASE_URL="postgres://$DB_USER:$DB_PASSWORD@$DB_HOST:$DB_PORT/$DB_NAME"

    echo -e "${GREEN}✓${NC} DATABASE_URL exported for this session"
    echo ""
    echo -e "${YELLOW}To persist this environment variable, add the following line to your shell profile:${NC}"
    echo -e "${GREEN}export DATABASE_URL=\"postgres://$DB_USER:$DB_PASSWORD@$DB_HOST:$DB_PORT/$DB_NAME\"${NC}"
    echo ""
    echo "For zsh (default on macOS), add to: ~/.zshrc"
    echo "For bash, add to: ~/.bashrc or ~/.bash_profile"
}

# Main execution
main() {
    # Check and install PostgreSQL
    if ! check_postgres; then
        install_postgres
    fi

    # Setup database and user
    setup_database

    # Check and install sqlx-cli
    if ! check_sqlx_cli; then
        install_sqlx_cli
    fi

    # Setup environment variable
    setup_env

    # Run migrations
    run_migrations

    echo ""
    echo -e "${GREEN}=== Database initialization complete! ===${NC}"
    echo ""
    echo "Database details:"
    echo "  Host: $DB_HOST"
    echo "  Port: $DB_PORT"
    echo "  Database: $DB_NAME"
    echo "  User: $DB_USER"
    echo "  Password: $DB_PASSWORD"
    echo ""
    echo "Connection string:"
    echo "  DATABASE_URL=postgres://$DB_USER:$DB_PASSWORD@$DB_HOST:$DB_PORT/$DB_NAME"
    echo ""
    echo -e "${YELLOW}Note: The DATABASE_URL is exported for this session only.${NC}"
    echo "Add it to your shell profile to make it permanent."
}

# Run main function
main
