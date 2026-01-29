# Authentication Flow - Sequence Diagram

This document contains a simplified Mermaid sequence diagram illustrating the complete authentication and authorization flow in the AOMI backend.

## Complete Authentication Flow

```mermaid
sequenceDiagram
    participant Client
    participant Middleware as API Key Middleware
    participant Endpoint as Endpoint Handler

    Client->>Middleware: HTTP Request<br/>(Path, Headers: X-Session-Id, X-API-Key)
    
    Note over Middleware: Step 1: Check if path starts with /api/
    alt Path does not start with /api/
        Middleware->>Endpoint: Skip middleware (Public endpoint)
        Endpoint->>Client: 200 OK
    else Path starts with /api/
        Note over Middleware: Step 2: Check if endpoint requires Session ID<br/>Required: /api/chat, /api/state, /api/interrupt,<br/>/api/updates, /api/system, /api/events,<br/>/api/memory-mode, /api/sessions/:id,<br/>/api/db/sessions/:id
        alt Endpoint requires Session ID
            alt X-Session-Id header missing
                Middleware->>Client: 400 BAD_REQUEST
            else X-Session-Id header present
                Middleware->>Middleware: Extract & validate SessionId
                Note over Middleware: Step 3: Check if endpoint requires API Key<br/>Only /api/chat with non-default namespace
                alt Endpoint is /api/chat with non-default namespace
                    alt X-API-Key header missing
                        Middleware->>Client: 401 UNAUTHORIZED
                    else X-API-Key header present
                        Note over Middleware: Validate API key & namespace authorization
                        alt API key invalid or not found
                            Middleware->>Client: 403 FORBIDDEN
                        else API key not authorized for namespace
                            Middleware->>Client: 403 FORBIDDEN
                        else API key valid and authorized
                            Middleware->>Middleware: Insert extensions:<br/>- SessionId<br/>- AuthorizedKey
                            Middleware->>Endpoint: Request with extensions
                            Endpoint->>Client: 200 OK
                        end
                    end
                else Endpoint does not require API Key
                    Middleware->>Middleware: Insert extension: SessionId
                    Middleware->>Endpoint: Request with SessionId extension
                    Endpoint->>Client: 200 OK
                end
            end
        else Endpoint does not require Session ID
            Middleware->>Endpoint: Request (no auth required)
            Endpoint->>Client: 200 OK
        end
    end
```
