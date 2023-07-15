import { NestFactory } from '@nestjs/core'
import { ClientModule } from './client.module'
import { WsAdapter } from '@nestjs/platform-ws'
import { ConfigService } from '@nestjs/config'

async function bootstrap() {
    const app = await NestFactory.create(ClientModule)
    const configService = app.get(ConfigService)
    const servers = configService.get('NATS_SERVER')
    console.log('servers', servers)
    app.useWebSocketAdapter(new WsAdapter(app))
    await app.listen(3000)
}
bootstrap()
