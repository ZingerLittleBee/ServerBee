import { NestFactory } from '@nestjs/core'
import { AuthModule } from './auth.module'
import { MicroserviceOptions, Transport } from '@nestjs/microservices'
import { ConfigService } from '@nestjs/config'
import {
    defaultAuthServicePort,
    defaultAuthServiceServerHost,
    kAuthServicePort,
    kAuthServiceServerHost
} from '@server-octopus/shared'

async function bootstrap() {
    const app = await NestFactory.create(AuthModule)
    const configService = app.get(ConfigService)
    const host =
        configService.get<string>(kAuthServiceServerHost) ??
        defaultAuthServiceServerHost
    const port =
        configService.get<number>(kAuthServicePort) ?? defaultAuthServicePort

    const microservice = app.connectMicroservice<MicroserviceOptions>({
        transport: Transport.TCP,
        options: {
            host,
            port
        }
    })
    await microservice.listen()
}
bootstrap()
