import { NestFactory } from '@nestjs/core'
import { ClientModule } from './client.module'
import { WsAdapter } from '@nestjs/platform-ws'
import * as cookieParser from 'cookie-parser'

async function bootstrap() {
    const app = await NestFactory.create(ClientModule)
    app.useWebSocketAdapter(new WsAdapter(app))
    app.use(cookieParser())
    await app.listen(3002)
}
bootstrap()
