import { NestFactory } from '@nestjs/core'
import { AppModule } from './app.module'
import * as cookieParser from 'cookie-parser'
import helmet from 'helmet'
import { NestExpressApplication } from '@nestjs/platform-express'

async function bootstrap() {
    const app = await NestFactory.create<NestExpressApplication>(AppModule, {
        rawBody: true,
        logger: ['error', 'warn', 'log', 'debug', 'verbose']
    })
    app.useBodyParser('text')
    app.use(helmet())
    app.use(cookieParser())
    app.enableCors({
        origin: 'http://localhost:3000',
        credentials: true
    })
    await app.listen(3001)
}
bootstrap()
