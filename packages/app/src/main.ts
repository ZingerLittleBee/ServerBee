import { NestFactory } from '@nestjs/core'
import { AppModule } from './app.module'
import * as cookieParser from 'cookie-parser'
import helmet from 'helmet'
import { NestExpressApplication } from '@nestjs/platform-express'
import { WsAdapter } from '@nestjs/platform-ws'

async function bootstrap() {
    const app = await NestFactory.create<NestExpressApplication>(AppModule, {
        logger: ['error', 'warn', 'log', 'debug', 'verbose']
    })
    app.useWebSocketAdapter(new WsAdapter(app))
    app.use(helmet())
    app.use(cookieParser())
    app.enableCors({
        origin: true,
        credentials: true
    })
    await app.listen(3001)
}
bootstrap()
