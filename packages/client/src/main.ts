import { NestFactory } from '@nestjs/core'
import { ClientModule } from './client.module'
import { WsAdapter } from '@nestjs/platform-ws'
async function bootstrap() {
    const app = await NestFactory.create(ClientModule)
    app.useWebSocketAdapter(new WsAdapter(app))
    await app.listen(3000)
}
bootstrap()
