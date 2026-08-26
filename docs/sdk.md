## SDK

Each language has a clean SDK that wraps the raw WIT bindings. Plugin authors never touch WIT directly.

**Rust**
```rust
use aether_sdk::{Db, Email, Events, Plugins};

Db::query::<Invoice>("invoice", "status = 'draft'")?;
Email::send("user@example.com", "Hello", "Body")?;
Events::emit("invoice.posted", &payload);
Plugins::call::<_, StockLevel>("inventory", "check-stock", &req)?;
```

**Python**
```python
from aether import Db, Email, Events, Plugins

Db.query("invoice", "status = 'draft'")
Email.send("user@example.com", "Hello", "Body")
Events.emit("invoice.posted", {"id": invoice_id})
Plugins.call("inventory", "check-stock", {"invoice_id": id})
```

**TypeScript**
```typescript
import { Db, Email, Events, Plugins } from '@aether/sdk';

await Db.query<Invoice>('invoice', "status = 'draft'");
Email.send('user@example.com', 'Hello', 'Body');
Events.emit('invoice.posted', { id: invoiceId });
Plugins.call<CheckRequest, StockLevel>('inventory', 'check-stock', req);
```

**Go**
```go
import "github.com/aether/sdk-go/db"
import "github.com/aether/sdk-go/events"

db.Query[Invoice]("invoice", "status = 'draft'")
events.Emit("invoice.posted", payload)
```
